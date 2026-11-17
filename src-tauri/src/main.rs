use espflash::{flasher::Flasher, interface::Interface};
use serde::{Deserialize, Serialize};
use serialport;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::Manager;
use tauri_plugin_shell;

// ---------------------------------------------------------------------------
// Firmware sources: GitHub releases, served through each repo's Cloudflare Worker
// (same endpoints as the web flasher).
// ---------------------------------------------------------------------------

const CHORDS_REPO: &str = "upsidedownlabs/Chords-Arduino-Firmware";
const CHORDS_WORKER: &str = "https://chords-arduino-firmware.myupsidedownlab.workers.dev";
const PROJECTS_REPO: &str = "upsidedownlabs/NPG-Lite-Arduino-Firmware";
const PROJECTS_WORKER: &str = "https://npg-flasher-web.myupsidedownlab.workers.dev";

/// (kind shown in the UI, release asset name)
const CHORDS_FILES: [(&str, &str); 3] = [
    ("BLE", "Chords-NPG-LITE-BLE-ESP32C6.merged.bin"),
    ("Serial", "Chords-NPG-LITE-Serial-ESP32C6.merged.bin"),
    ("WiFi", "Chords-NPG-LITE-WiFi-ESP32C6.merged.bin"),
];

/// Merged images cover the whole 4 MB flash and start with the ESP image magic byte.
const MERGED_IMAGE_SIZE: usize = 4 * 1024 * 1024;
const ESP_IMAGE_MAGIC: u8 = 0xE9;
const HTTP_USER_AGENT: &str = "NPG-Lite-Flasher";

#[derive(Deserialize)]
struct WorkerAsset {
    name: String,
    url: String,
}

#[derive(Deserialize)]
struct WorkerLatest {
    tag: String,
    assets: Option<Vec<WorkerAsset>>,
}

struct LatestRelease {
    tag: String,
    /// (asset name, download url)
    assets: Vec<(String, String)>,
}

impl LatestRelease {
    fn asset_url(&self, name: &str) -> Option<&str> {
        self.assets
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, url)| url.as_str())
    }
}

/// Latest release of `repo`: the Worker's /latest first, GitHub's API as fallback.
/// Returns None when neither answers (offline).
async fn latest_release(repo: &str, worker: &str) -> Option<LatestRelease> {
    let client = reqwest::Client::builder()
        .user_agent(HTTP_USER_AGENT)
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;

    if let Ok(resp) = client.get(format!("{}/latest", worker)).send().await {
        if resp.status().is_success() {
            if let Ok(latest) = resp.json::<WorkerLatest>().await {
                if let Some(assets) = latest.assets {
                    if !latest.tag.is_empty() {
                        let assets = assets
                            .into_iter()
                            .map(|a| {
                                let url = if a.url.starts_with("http://") || a.url.starts_with("https://") {
                                    a.url
                                } else if a.url.starts_with('/') {
                                    format!("{}{}", worker, a.url)
                                } else {
                                    format!("{}/{}", worker, a.url)
                                };
                                (a.name, url)
                            })
                            .collect();
                        return Some(LatestRelease { tag: latest.tag, assets });
                    }
                }
            }
        }
    }

    let resp = client
        .get(format!("https://api.github.com/repos/{}/releases/latest", repo))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let release: serde_json::Value = resp.json().await.ok()?;
    let tag = release["tag_name"].as_str()?.to_string();
    let assets = release["assets"]
        .as_array()?
        .iter()
        .filter_map(|asset| {
            let name = asset["name"].as_str()?.to_string();
            let url = format!("{}/firmware/{}/{}", worker, tag, name);
            Some((name, url))
        })
        .collect();
    Some(LatestRelease { tag, assets })
}

/// Downloads a merged firmware image to `dest`. The file is only replaced when the
/// download is complete and looks like a 4 MB ESP image.
async fn download_firmware(url: &str, dest: &Path) -> Result<(), String> {
    let file_name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| format!("Invalid firmware path {:?}", dest))?;

    let client = reqwest::Client::builder()
        .user_agent(HTTP_USER_AGENT)
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to download {}: {}", file_name, e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("Failed to download {}: server returned {}", file_name, status));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to download {}: {}", file_name, e))?;

    if bytes.len() != MERGED_IMAGE_SIZE {
        return Err(format!(
            "Downloaded {} is {} bytes, expected {} bytes (a 4 MB merged image)",
            file_name,
            bytes.len(),
            MERGED_IMAGE_SIZE
        ));
    }
    if bytes[0] != ESP_IMAGE_MAGIC {
        return Err(format!(
            "Downloaded {} is not an ESP firmware image (first byte 0x{:02X}, expected 0x{:02X})",
            file_name, bytes[0], ESP_IMAGE_MAGIC
        ));
    }

    let dir = dest
        .parent()
        .ok_or_else(|| format!("Invalid firmware path {:?}", dest))?;
    fs::create_dir_all(dir).map_err(|e| format!("Failed to create {:?}: {}", dir, e))?;

    // Write next to the target, then rename, so a failed write never replaces a good file.
    let tmp = dir.join(format!("{}.part", file_name));
    if let Err(e) = fs::write(&tmp, &bytes) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Failed to save {}: {}", file_name, e));
    }
    if let Err(e) = fs::rename(&tmp, dest) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("Failed to save {}: {}", file_name, e));
    }
    Ok(())
}

/// Rejects names that could escape the firmware folder.
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\\') && name != "." && name != ".."
}

fn app_files_path(app: &tauri::AppHandle, relative: &str) -> Result<PathBuf, String> {
    app.path()
        .resolve(format!("files/{}", relative), tauri::path::BaseDirectory::AppData)
        .map_err(|e| format!("Failed to resolve files/{}: {}", relative, e))
}

// ---------------------------------------------------------------------------
// files/firmware-versions.json: "chords/<file>" or "custom/<file>" -> release tag
// ---------------------------------------------------------------------------

type Versions = BTreeMap<String, String>;

static VERSIONS_LOCK: Mutex<()> = Mutex::new(());

fn read_versions(path: &Path) -> Versions {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn load_versions(app: &tauri::AppHandle) -> Versions {
    let _guard = VERSIONS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    app_files_path(app, "firmware-versions.json")
        .map(|p| read_versions(&p))
        .unwrap_or_default()
}

fn update_versions(app: &tauri::AppHandle, change: impl FnOnce(&mut Versions)) -> Result<(), String> {
    let _guard = VERSIONS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = app_files_path(app, "firmware-versions.json")?;
    let mut versions = read_versions(&path);
    change(&mut versions);

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Failed to create {:?}: {}", dir, e))?;
    }
    let json = serde_json::to_string_pretty(&versions)
        .map_err(|e| format!("Failed to encode firmware versions: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("Failed to save firmware versions: {}", e))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Failed to save firmware versions: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Chords firmware (BLE / Serial / WiFi), kept in AppData/files/chords/
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChordsFirmwareStatus {
    kind: String,
    file: String,
    available: bool,
    tag: Option<String>,
    /// Was missing and has just been downloaded.
    downloaded: bool,
    /// Existed and has just been replaced by a newer release.
    updated: bool,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChordsSyncResult {
    online: bool,
    latest_tag: Option<String>,
    firmwares: Vec<ChordsFirmwareStatus>,
}

/// Only one Chords sync at a time, so two calls never download the same file together.
static CHORDS_SYNC_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tauri::command]
async fn sync_chords_firmwares(app: tauri::AppHandle) -> Result<ChordsSyncResult, String> {
    let _sync = CHORDS_SYNC_LOCK.lock().await;
    let dir = app_files_path(&app, "chords")?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create {:?}: {}", dir, e))?;

    let latest = latest_release(CHORDS_REPO, CHORDS_WORKER).await;
    let versions = load_versions(&app);
    let mut firmwares = Vec::new();

    for (kind, file) in CHORDS_FILES {
        let path = dir.join(file);
        let key = format!("chords/{}", file);
        let exists = path.is_file();
        let saved_tag = versions.get(&key).cloned();

        let mut status = ChordsFirmwareStatus {
            kind: kind.to_string(),
            file: file.to_string(),
            available: exists,
            tag: saved_tag.clone(),
            downloaded: false,
            updated: false,
            error: None,
        };

        // Offline: keep whatever is saved.
        if let Some(release) = &latest {
            if !exists || saved_tag.as_deref() != Some(release.tag.as_str()) {
                match release.asset_url(file) {
                    None => {
                        status.error = Some(format!("{} is not in Chords release {}", file, release.tag));
                    }
                    Some(url) => match download_firmware(url, &path).await {
                        Ok(()) => {
                            status.available = true;
                            status.tag = Some(release.tag.clone());
                            if exists {
                                status.updated = true;
                            } else {
                                status.downloaded = true;
                            }
                            if let Err(e) = update_versions(&app, |v| {
                                v.insert(key.clone(), release.tag.clone());
                            }) {
                                status.error = Some(e);
                            }
                        }
                        Err(e) => status.error = Some(e),
                    },
                }
            }
        }

        firmwares.push(status);
    }

    Ok(ChordsSyncResult {
        online: latest.is_some(),
        latest_tag: latest.map(|r| r.tag),
        firmwares,
    })
}

// ---------------------------------------------------------------------------
// Project firmware (NPG-Lite-Arduino-Firmware), saved in AppData/files/custom/
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFirmwareList {
    tag: String,
    /// [name, url] pairs, as the UI already uses them.
    firmwares: Vec<(String, String)>,
}

#[tauri::command]
async fn list_project_firmwares() -> Result<ProjectFirmwareList, String> {
    let release = latest_release(PROJECTS_REPO, PROJECTS_WORKER)
        .await
        .ok_or("Could not reach the firmware server. Check your internet connection.")?;

    let firmwares: Vec<(String, String)> = release
        .assets
        .into_iter()
        .filter(|(name, _)| name.ends_with(".ino.merged.bin"))
        .collect();

    if firmwares.is_empty() {
        return Err(format!("No .ino.merged.bin files found in release {}", release.tag));
    }

    Ok(ProjectFirmwareList { tag: release.tag, firmwares })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectUpdate {
    /// "updated", "upToDate", "offline", "untracked", "notInRelease" or "failed"
    status: &'static str,
    tag: Option<String>,
    previous_tag: Option<String>,
    error: Option<String>,
}

#[tauri::command]
async fn ensure_latest_project(app: tauri::AppHandle, name: String) -> Result<ProjectUpdate, String> {
    if !is_plain_file_name(&name) {
        return Err(format!("Invalid firmware name: {}", name));
    }
    let key = format!("custom/{}", name);
    let previous_tag = load_versions(&app).get(&key).cloned();

    let result = |status, tag: Option<String>, error: Option<String>| ProjectUpdate {
        status,
        tag,
        previous_tag: previous_tag.clone(),
        error,
    };

    // No recorded tag: a user upload. Leave it alone.
    let Some(saved_tag) = previous_tag.clone() else {
        return Ok(result("untracked", None, None));
    };

    let Some(release) = latest_release(PROJECTS_REPO, PROJECTS_WORKER).await else {
        return Ok(result("offline", Some(saved_tag), None));
    };

    if release.tag == saved_tag {
        return Ok(result("upToDate", Some(saved_tag), None));
    }

    let Some(url) = release.asset_url(&name) else {
        return Ok(result("notInRelease", Some(saved_tag), None));
    };

    let path = app_files_path(&app, "custom")?.join(&name);
    if let Err(e) = download_firmware(url, &path).await {
        return Ok(result("failed", Some(saved_tag), Some(e)));
    }
    if let Err(e) = update_versions(&app, |v| {
        v.insert(key.clone(), release.tag.clone());
    }) {
        return Ok(result("failed", Some(saved_tag), Some(e)));
    }

    Ok(result("updated", Some(release.tag), None))
}

#[tauri::command]
fn list_serial_ports() -> Vec<String> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|port| match port.port_type {
            serialport::SerialPortType::UsbPort(_) => Some(port.port_name),
            _ => None,
        })
        .collect()
}

#[tauri::command]
async fn list_custom_firmwares(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let firmware_dir = app.path()
        .resolve("files/custom", tauri::path::BaseDirectory::AppData)
        .map_err(|e| format!("Failed to resolve custom firmware path: {}", e))?;
    
    // Create directory if it doesn't exist
    if !firmware_dir.exists() {
        fs::create_dir_all(&firmware_dir)
            .map_err(|e| format!("Failed to create custom firmware directory: {}", e))?;
    }
    
    let mut firmwares = Vec::new();
    for entry in fs::read_dir(firmware_dir)
        .map_err(|e| format!("Failed to read custom firmware directory: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            if let Some(ext) = entry.path().extension() {
                if ext == "bin" {
                    if let Some(name) = entry.path().file_name() {
                        firmwares.push(name.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    
    Ok(firmwares)
}

#[tauri::command]
async fn save_custom_firmware(app: tauri::AppHandle, name: String, data: Vec<u8>) -> Result<String, String> {
    let firmware_dir = app.path()
        .resolve("files/custom", tauri::path::BaseDirectory::AppData)
        .map_err(|e| format!("Failed to resolve custom firmware path: {}", e))?;
    
    // Ensure the filename ends with .bin
    let filename = if name.ends_with(".bin") {
        name
    } else {
        format!("{}.bin", name)
    };
    
    let path = firmware_dir.join(&filename);
    
    fs::write(&path, &data)
        .map_err(|e| format!("Failed to save firmware {}: {}", filename, e))?;

    // A user upload has no release tag; drop any tag left by a downloaded file of the
    // same name so ensure_latest_project never overwrites the upload.
    update_versions(&app, |v| {
        v.remove(&format!("custom/{}", filename));
    })?;

    Ok(filename)
}

#[tauri::command]
async fn flash_firmware(app: tauri::AppHandle, portname: String, file: String, iscustom: bool) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        use std::{fs::File, io::Read, thread::sleep, time::Duration};

        let read_bin = |path: &std::path::Path| -> Result<Vec<u8>, String> {
            let mut file = File::open(path).map_err(|e| format!("Failed to open {:?}: {}", path, e))?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)
                .map_err(|e| format!("Failed to read {:?}: {}", path, e))?;
            if buffer.is_empty() {
                return Err(format!("File {:?} is empty", path));
            }
            Ok(buffer)
        };

        let resource_path = if iscustom {
            app.path()
                .resolve(&format!("files/custom/{}", file), tauri::path::BaseDirectory::AppData)
                .map_err(|e| format!("Failed to resolve custom firmware path: {}", e))?
        } else {
            app.path()
                .resolve(&format!("files/chords/{}", file), tauri::path::BaseDirectory::AppData)
                .map_err(|e| format!("Failed to resolve Chords firmware path: {}", e))?
        };

        let app = read_bin(&resource_path)?;

        let ports = serialport::available_ports().map_err(|e| {
            format!("Failed to list ports: {}. Try 'sudo usermod -a -G dialout $USER' and reboot", e)
        })?;

        let port_info = ports
            .iter()
            .find(|p| p.port_name == portname)
            .ok_or_else(|| {
                let available = ports.iter().map(|p| &p.port_name).collect::<Vec<_>>();
                format!("Port {} not found. Available ports: {:?}", portname, available)
            })?;

        let usb_info = match &port_info.port_type {
            serialport::SerialPortType::UsbPort(info) => info,
            _ => return Err(format!("Port {} is not a USB port", portname)),
        };

        println!("Connecting to interface...");
        let interface = (0..3)
            .find_map(|attempt| {
                let result = Interface::new(&port_info.clone(), Some(1), Some(1))
                    .map_err(|e| format!("Attempt {}: {}", attempt + 1, e));
                if result.is_ok() || attempt == 2 {
                    Some(result)
                } else {
                    sleep(Duration::from_millis(500));
                    None
                }
            })
            .unwrap_or_else(|| Err("Failed to create interface after 3 attempts".into()))?;

        let mut flasher = Flasher::connect(interface, usb_info.clone(), Some(921600), false)
            .map_err(|e| format!("Failed to connect to device: {}", e))?;

        fn retry_flash(
            flasher: &mut Flasher,
            addr: u32,
            data: &[u8],
            name: &str,
        ) -> Result<(), String> {
            for i in 0..5 {
                match flasher.write_bin_to_flash(addr, data, None) {
                    Ok(_) => {
                        println!("{} flashed successfully (attempt {}).", name, i + 1);
                        return Ok(());
                    }
                    Err(e) => {
                        if i == 4 {
                            return Err(format!(
                                "{} flash failed at 0x{:X} ({} bytes): {}",
                                name, addr, data.len(), e
                            ));
                        }
                        println!("Retrying {} flash (attempt {})...", name, i + 2);
                        sleep(Duration::from_millis(500));
                    }
                }
            }
            Err(format!("{} flash failed after 5 retries", name))
        }

        println!("Flashing application...");
        retry_flash(&mut flasher, 0x10000, &app, "Application")?;
        sleep(Duration::from_millis(500));

        Ok("✅ Firmware flashed successfully! The device should now reboot.".to_string())
    })
    .await
    .map_err(|e| format!("Thread error: {:?}", e))?
}
#[tauri::command]
async fn delete_custom_firmware(app: tauri::AppHandle, filename: String) -> Result<String, String> {
    let firmware_dir = app.path()
        .resolve("files/custom", tauri::path::BaseDirectory::AppData)
        .map_err(|e| format!("Failed to resolve custom firmware path: {}", e))?;
    
    let path = firmware_dir.join(&filename);
    
    if !path.exists() {
        return Err(format!("Firmware file {} does not exist", filename));
    }
    
    std::fs::remove_file(&path)
        .map_err(|e| format!("Failed to delete firmware {}: {}", filename, e))?;

    // The file is gone either way; a stale entry would only be cosmetic, so just log it.
    if let Err(e) = update_versions(&app, |v| {
        v.remove(&format!("custom/{}", filename));
    }) {
        eprintln!("Deleted {} but could not update firmware versions: {}", filename, e);
    }

    Ok(format!("Firmware {} deleted successfully", filename))
}

#[tauri::command]
async fn download_and_save_firmware(app: tauri::AppHandle, url: String, name: String, tag: String) -> Result<String, String> {
    if !is_plain_file_name(&name) {
        return Err(format!("Invalid firmware name: {}", name));
    }

    let file_path = app_files_path(&app, "custom")?.join(&name);
    download_firmware(&url, &file_path).await?;

    update_versions(&app, |v| {
        v.insert(format!("custom/{}", name), tag.clone());
    })?;

    Ok(name)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            sync_chords_firmwares,
            list_project_firmwares,
            ensure_latest_project,
            download_and_save_firmware,
            list_serial_ports,
            flash_firmware,
            list_custom_firmwares,
            save_custom_firmware,
            delete_custom_firmware
        ])
        .plugin(tauri_plugin_shell::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}