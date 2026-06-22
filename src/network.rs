use std::process::Command;
use std::os::windows::process::CommandExt;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use serde::Deserialize;

static ADAPTER_CACHE: Mutex<Option<(Instant, Vec<Adapter>)>> = Mutex::new(None);
const CACHE_TTL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Deserialize)]
pub struct Adapter {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "InterfaceDescription")]
    pub description: String,
    #[serde(rename = "Status")]
    pub status: String,
}

pub fn get_adapters() -> Vec<Adapter> {
    if let Ok(mut cache) = ADAPTER_CACHE.lock() {
        if let Some((ts, ref adapters)) = *cache {
            if ts.elapsed() < CACHE_TTL {
                return adapters.clone();
            }
        }
    }

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command",
            "Get-NetAdapter | Select-Object Name, InterfaceDescription, Status | ConvertTo-Json"])
        .creation_flags(0x08000000)
        .output();

    let adapters = match output {
        Ok(out) if out.status.success() => {
            let (text, _, _) = encoding_rs::GBK.decode(&out.stdout);
            if text.trim().is_empty() {
                return vec![];
            }
            serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| {
                    if v.is_array() {
                        serde_json::from_value::<Vec<Adapter>>(v).ok()
                    } else {
                        serde_json::from_value::<Adapter>(v).ok().map(|a| vec![a])
                    }
                })
                .unwrap_or_default()
        }
        _ => vec![],
    };

    if let Ok(mut cache) = ADAPTER_CACHE.lock() {
        *cache = Some((Instant::now(), adapters.clone()));
    }

    adapters
}

pub const VIRTUAL_KEYWORDS: &[&str] = &[
    "tailscale", "wireguard", "openvpn", "vpn", "virtualbox", "vmware",
    "hyper-v", "docker", "loopback", "teredo", "isatap", "6to4",
    "bluetooth", "debug", "npcap",
];

pub fn is_phone_active(adapters: &[Adapter], campus_names: &[&str]) -> bool {
    adapters.iter().any(|a| {
        if a.status != "Up" { return false; }
        if campus_names.contains(&a.name.as_str()) { return false; }
        let name_lower = a.name.to_lowercase();
        let desc_lower = a.description.to_lowercase();
        if VIRTUAL_KEYWORDS.iter().any(|k| name_lower.contains(k) || desc_lower.contains(k)) {
            return false;
        }
        true
    })
}

pub fn get_active_adapter(adapters: &[Adapter], ethernet_name: &str, wifi_name: &str) -> Option<(String, String)> {
    for a in adapters {
        if a.name == ethernet_name && a.status == "Up" {
            return Some(("ethernet".into(), ethernet_name.into()));
        }
    }
    for a in adapters {
        if a.name == wifi_name && a.status == "Up" {
            return Some(("wifi".into(), wifi_name.into()));
        }
    }
    None
}

pub fn is_adapter_up(adapters: &[Adapter], name: &str) -> bool {
    adapters.iter().any(|a| a.name == name && a.status == "Up")
}

pub fn set_adapter(name: &str, enable: bool) -> bool {
    let action = if enable { "enable" } else { "disable" };
    let cmd = format!("netsh interface set interface \"{}\" admin={}", name, action);
    let output = Command::new("cmd")
        .args(["/C", &cmd])
        .creation_flags(0x08000000)
        .output();
    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

pub fn is_at_school(gateway: &str) -> bool {
    let addr = format!("{}:801", gateway);
    std::net::TcpStream::connect_timeout(
        &addr.parse().unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap()),
        std::time::Duration::from_secs(1),
    ).is_ok()
}

pub fn detect_ethernet_name(adapters: &[Adapter]) -> Option<String> {
    let keywords = ["ethernet", "以太网", "realtek", "intel", "killer"];
    let exclude = ["wireless", "wi-fi", "wifi"];
    adapters.iter().find(|a| {
        let desc = a.description.to_lowercase();
        keywords.iter().any(|k| desc.contains(k)) && !exclude.iter().any(|k| desc.contains(k))
    }).map(|a| a.name.clone())
}

pub fn detect_wifi_name(adapters: &[Adapter]) -> Option<String> {
    let keywords = ["wireless", "wi-fi", "wifi", "wlan", "无线"];
    adapters.iter().find(|a| {
        let desc = a.description.to_lowercase();
        keywords.iter().any(|k| desc.contains(k))
    }).map(|a| a.name.clone())
}

pub fn detect_gateway() -> Option<String> {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command",
            "Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Select-Object -First 1 -ExpandProperty NextHop"])
        .creation_flags(0x08000000)
        .output();

    output.ok()
        .and_then(|o| {
            let (s, _, _) = encoding_rs::GBK.decode(&o.stdout);
            Some(s.trim().to_string())
        })
        .filter(|s| !s.is_empty() && s != "0.0.0.0" && s != "::")
}

pub fn detect_ac_ip(gateway: &str) -> Option<String> {
    let parts: Vec<&str> = gateway.rsplitn(2, '.').collect();
    if parts.len() == 2 {
        let ac_ip = format!("{}.254", parts[1]);
        let addr = format!("{}:801", ac_ip);
        if std::net::TcpStream::connect_timeout(
            &addr.parse().unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap()),
            std::time::Duration::from_secs(1),
        ).is_ok() {
            return Some(ac_ip);
        }
    }
    None
}

pub fn auto_detect_all() -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    let adapters = get_adapters();
    let eth = detect_ethernet_name(&adapters);
    let wifi = detect_wifi_name(&adapters);
    let gw = detect_gateway();
    let ac = gw.as_deref().and_then(detect_ac_ip);
    (eth, wifi, gw, ac)
}

use std::sync::OnceLock;

static HTTP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();

pub fn get_http_client() -> &'static reqwest::blocking::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new())
    })
}

pub fn check_internet() -> bool {
    get_http_client()
        .head("https://www.baidu.com")
        .send()
        .ok()
        .filter(|r| r.status().is_success())
        .is_some()
}
