use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub student_id: String,
    pub password: String,
    pub ethernet_name: String,
    pub wifi_name: String,
    pub gateway: String,
    pub ac_ip: String,
    pub force_ethernet_priority: bool,
    pub auto_start: bool,
    pub theme: String,
    pub base_retry_interval: u64,
    pub max_retry_interval: u64,
    pub normal_check_interval: u64,
    pub mutex_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            student_id: "000000000000".into(),
            password: "000000".into(),
            ethernet_name: "\u{4ee5}\u{592a}\u{7f51}".into(), // 以太网
            wifi_name: "WLAN".into(),
            gateway: "10.20.3.1".into(),
            ac_ip: "10.20.3.254".into(),
            force_ethernet_priority: true,
            auto_start: false,
            theme: "light".into(),
            base_retry_interval: 2,
            max_retry_interval: 300,
            normal_check_interval: 5,
            mutex_port: 65432,
        }
    }
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_path() -> PathBuf {
    exe_dir().join("config.json")
}

pub fn log_path() -> PathBuf {
    exe_dir().join("guardian_activity.log")
}

pub fn load_config() -> Config {
    let path = config_path();
    if path.exists() {
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<Config>(&data) {
                return cfg;
            }
        }
    }
    Config::default()
}

pub fn save_config(cfg: &Config) {
    let path = config_path();
    if let Ok(data) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, data);
    }
    apply_auto_start(cfg.auto_start);
}

pub fn save_config_no_registry(cfg: &Config) {
    let path = config_path();
    if let Ok(data) = serde_json::to_string_pretty(cfg) {
        let _ = std::fs::write(path, data);
    }
}

pub fn uninstall_all() {
    apply_auto_start(false);
    let _ = std::fs::remove_file(config_path());
    let _ = std::fs::remove_file(log_path());
}

const REG_RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const APP_NAME: &str = "CampusNetGuardian";

pub fn apply_auto_start(enable: bool) {
    unsafe {
        use windows_sys::Win32::System::Registry::*;
        use windows_sys::Win32::Foundation::*;

        let key_path: Vec<u16> = REG_RUN_KEY.encode_utf16().chain(std::iter::once(0)).collect();
        let name: Vec<u16> = APP_NAME.encode_utf16().chain(std::iter::once(0)).collect();

        let mut hkey: HKEY = std::ptr::null_mut();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            key_path.as_ptr(),
            0,
            KEY_SET_VALUE | KEY_READ,
            &mut hkey,
        );

        if result != ERROR_SUCCESS { return; }

        if enable {
            let exe_path = std::env::current_exe()
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let value: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
            RegSetValueExW(
                hkey,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr() as *const u8,
                (value.len() * 2) as u32,
            );
        } else {
            RegDeleteValueW(hkey, name.as_ptr());
        }

        RegCloseKey(hkey);
    }
}
