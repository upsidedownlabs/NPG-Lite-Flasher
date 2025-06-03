use espflash::{flasher::Flasher, interface::Interface};
use serialport;
use tauri::Manager;
use std::fs;

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
                .resolve(&format!("files/{}", file), tauri::path::BaseDirectory::Resource)
                .map_err(|e| format!("Failed to resolve resource path: {}", e))?
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
    
    Ok(format!("Firmware {} deleted successfully", filename))
}

#[tauri::command]
async fn fetch_github_releases(repo: String) -> Result<Vec<(String, String)>, String> {
    let client = reqwest::Client::new();
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    
    let response = client
        .get(&url)
        .header("User-Agent", "Tauri App")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch releases: {}", e))?;
    
    if !response.status().is_success() {
        return Err(format!("GitHub API returned status: {}", response.status()));
    }
    
    let release: serde_json::Value = response.json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    
    let assets = release["assets"].as_array()
        .ok_or("No assets found in release")?;
    
    let firmwares: Vec<(String, String)> = assets.iter()
        .filter_map(|asset| {
            let name = asset["name"].as_str()?.to_string();
            if name.ends_with(".bin") {
                Some((name, asset["browser_download_url"].as_str()?.to_string()))
            } else {
                None
            }
        })
        .collect();
    
    if firmwares.is_empty() {
        return Err("No .bin files found in release assets".into());
    }
    
    Ok(firmwares)
}
#[tauri::command]
async fn download_and_save_firmware(app: tauri::AppHandle, url: String, name: String) -> Result<String, String> {
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "Tauri App")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch firmware: {}", e))?;

    let bytes = response.bytes().await.map_err(|e| format!("Failed to read response body: {}", e))?;

    let firmware_dir = app.path()
        .resolve("files/custom", tauri::path::BaseDirectory::AppData)
        .map_err(|e| format!("Failed to resolve custom firmware path: {}", e))?;

    // Create the directory if it doesn't exist
    std::fs::create_dir_all(&firmware_dir).map_err(|e| format!("Failed to create firmware directory: {}", e))?;

    let file_path = firmware_dir.join(&name);
    std::fs::write(&file_path, bytes).map_err(|e| format!("Failed to write firmware file: {}", e))?;

    Ok(name)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            fetch_github_releases,
            download_and_save_firmware,
            list_serial_ports, 
            flash_firmware,
            list_custom_firmwares,
            save_custom_firmware,
            delete_custom_firmware  // Add this line
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}