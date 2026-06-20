use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::config::{self, Config};
use crate::network;
use crate::auth;

#[derive(Debug, Clone)]
pub enum GuardianState {
    Initializing,
    Connected { adapter: String },
    Disconnected,
    Retrying { adapter: String, interval: u64, next_retry: f64 },
    Phone { adapter: String },
    Away,
    Error,
    Paused,
}

pub struct GuardianThread {
    pub config: Arc<Mutex<Config>>,
    pub log_tx: mpsc::Sender<String>,
    pub state_tx: mpsc::Sender<GuardianState>,
    pub stop: Arc<AtomicBool>,
    pub pause: Arc<AtomicBool>,
    pub last_heartbeat: Arc<Mutex<Instant>>,
    pub disabled_adapter: Arc<Mutex<Option<String>>>,
    pub dry_run: bool,
}

impl GuardianThread {
    pub fn run(&self) {
        if self.dry_run {
            self.log("[测试模式] 守护线程启动，只检测不操作。");
        } else {
            self.log("CampusNet Guardian V1.0.0 已启动。");
        }
        self.send_state(GuardianState::Initializing);

        let mut current_interval = {
            let cfg = self.config.lock().unwrap();
            cfg.base_retry_interval
        };

        loop {
            if self.stop.load(Ordering::Relaxed) {
                break;
            }

            *self.last_heartbeat.lock().unwrap() = Instant::now();

            if self.pause.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }

            let cfg = self.config.lock().unwrap().clone();
            let campus_names = [&*cfg.ethernet_name, &*cfg.wifi_name];

            let adapters = network::get_adapters();
            let phone_active = network::is_phone_active(&adapters, &campus_names);

            if phone_active {
                let mut disabled = self.disabled_adapter.lock().unwrap();
                if disabled.is_none() {
                    if let Some((_, name)) = network::get_active_adapter(&adapters, &cfg.ethernet_name, &cfg.wifi_name) {
                        if self.dry_run {
                            self.log(&format!("[测试] 检测到USB共享设备，将禁用 {}。", name));
                        } else {
                            self.log(&format!("检测到USB共享设备，禁用 {}。", name));
                            network::set_adapter(&name, false);
                        }
                        *disabled = Some(name);
                    } else {
                        *disabled = Some("__none__".into());
                    }
                    self.send_state(GuardianState::Phone { adapter: "USB".into() });
                    self.wait_for_network_ready(&cfg);
                } else {
                    self.send_state(GuardianState::Phone { adapter: "USB".into() });
                    self.interruptible_sleep(Duration::from_secs(2));
                }
                continue;
            }

            {
                let mut disabled = self.disabled_adapter.lock().unwrap();
                if let Some(ref name) = disabled.clone() {
                    if name != "__none__" {
                        if self.dry_run {
                            self.log(&format!("[测试] USB设备已断开，将启用 {}。", name));
                        } else {
                            self.log(&format!("USB设备已断开，启用 {}。", name));
                            network::set_adapter(name, true);
                            self.wait_for_adapter_ready(name, &cfg);
                        }
                    }
                    *disabled = None;
                }
            }

            if network::check_internet() {
                current_interval = cfg.base_retry_interval;

                // 双网卡检测
                let eth_up = adapters.iter().any(|a| a.name == cfg.ethernet_name && a.status == "Up");
                let wifi_up = adapters.iter().any(|a| a.name == cfg.wifi_name && a.status == "Up");
                if eth_up && wifi_up && cfg.force_ethernet_priority {
                    self.log("有线与无线同时在线，建议使用有线。");
                }

                let adapter = network::get_active_adapter(&adapters, &cfg.ethernet_name, &cfg.wifi_name)
                    .map(|(_, n)| n)
                    .unwrap_or_else(|| "未知".into());
                self.send_state(GuardianState::Connected { adapter });
                self.interruptible_sleep(Duration::from_secs(cfg.normal_check_interval));
            } else if network::is_at_school(&cfg.gateway) {
                if let Some((_, name)) = network::get_active_adapter(&adapters, &cfg.ethernet_name, &cfg.wifi_name) {
                    if self.dry_run {
                        self.log("[测试] 链路中断，将发送认证请求。");
                    } else {
                        self.log("链路中断，发送认证请求。");
                        let (_ok, msg) = auth::do_login(&cfg.gateway, &cfg.student_id, &cfg.password, &cfg.ac_ip);
                        self.log(&msg);
                    }

                    let (actual_sleep, new_interval) = if current_interval >= cfg.max_retry_interval {
                        self.log(&format!("重试间隔已达上限 ({}s)，重置。", cfg.max_retry_interval));
                        (cfg.base_retry_interval, cfg.base_retry_interval)
                    } else {
                        let jitter = rand::random_range(-0.5f64..0.5);
                        let sleep = current_interval as f64 + jitter;
                        let next = (current_interval * 2).min(cfg.max_retry_interval + 1);
                        self.log(&format!("等待 {:.1}s 后重试。", sleep));
                        (sleep as u64, next)
                    };
                    current_interval = new_interval;

                    self.send_state(GuardianState::Retrying {
                        adapter: name,
                        interval: current_interval,
                        next_retry: actual_sleep as f64,
                    });
                    self.interruptible_sleep(Duration::from_secs(actual_sleep.max(1)));
                } else {
                    self.send_state(GuardianState::Disconnected);
                    self.interruptible_sleep(Duration::from_secs(10));
                }
            } else {
                self.send_state(GuardianState::Away);
                self.interruptible_sleep(Duration::from_secs(30));
            }
        }
    }

    fn log(&self, msg: &str) {
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let line = format!("[{}] {}", ts, msg);
        let _ = self.log_tx.send(line.clone());
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true)
            .open(config::log_path())
        {
            use std::io::Write;
            let _ = writeln!(f, "{}", line);
        }
    }

    fn send_state(&self, state: GuardianState) {
        let _ = self.state_tx.send(state);
    }

    fn interruptible_sleep(&self, duration: Duration) {
        let end = Instant::now() + duration;
        while Instant::now() < end {
            if self.stop.load(Ordering::Relaxed) {
                return;
            }
            if !self.pause.load(Ordering::Relaxed) && self.should_check_phone() {
                let cfg = self.config.lock().unwrap().clone();
                let campus_names = [&*cfg.ethernet_name, &*cfg.wifi_name];
                let adapters = network::get_adapters();
                if network::is_phone_active(&adapters, &campus_names) {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(2000));
        }
    }

    fn should_check_phone(&self) -> bool {
        self.disabled_adapter.lock().unwrap().is_none()
    }

    fn wait_for_network_ready(&self, _cfg: &Config) {
        for i in 0..15 {
            if self.stop.load(Ordering::Relaxed) { return; }
            if network::check_internet() {
                self.log(&format!("USB网络就绪 ({}s)。", i + 1));
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        self.log("USB网络就绪超时。");
    }

    fn wait_for_adapter_ready(&self, name: &str, cfg: &Config) {
        for i in 0..10 {
            if self.stop.load(Ordering::Relaxed) { return; }
            let adapters = network::get_adapters();
            if network::is_adapter_up(&adapters, name) && network::is_at_school(&cfg.gateway) {
                self.log(&format!("{} 就绪 ({}s)。", name, i + 1));
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        self.log(&format!("{} 就绪超时。", name));
    }
}
