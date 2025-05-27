use espflash::{flasher::Flasher, interface::Interface};
use serialport;
use tauri::Manager;

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
async fn flash_firmware(app: tauri::AppHandle, portname: String, file: String) -> Result<String, String> {
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

        let resource_path = app
            .path()
            .resolve(&format!("files/{}", file), tauri::path::BaseDirectory::Resource)
            .map_err(|e| format!("Failed to resolve resource path: {}", e))?;

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
            Err(format!("{} flash failed after 3 retries", name))
        }

        println!("Flashing application...");
        retry_flash(&mut flasher, 0x10000, &app, "Application")?;
        sleep(Duration::from_millis(500));

        Ok("✅ Firmware flashed successfully! The device should now reboot.".to_string())
    })
    .await
    .map_err(|e| format!("Thread error: {:?}", e))?
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![list_serial_ports, flash_firmware])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
