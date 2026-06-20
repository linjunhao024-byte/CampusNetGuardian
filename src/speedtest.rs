use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const TEST_URLS: &[&str] = &[
    "https://mirrors.tuna.tsinghua.edu.cn/ubuntu/ls-lR.gz",
    "https://mirrors.aliyun.com/ubuntu/ls-lR.gz",
    "https://repo.huaweicloud.com/ubuntu/ls-lR.gz",
];

pub enum TestEvent {
    PingResult { target: String, latency_ms: u64 },
    PingError { target: String, error: String },
    PingDone,
    SpeedProgress { downloaded: u64, elapsed_ms: u64, speed_mbps: f64 },
    SpeedDone { speed_mbps: f64, downloaded: u64, elapsed_ms: u64 },
    SpeedError(String),
}

pub fn get_current_network_info(ethernet_name: &str, wifi_name: &str) -> (String, String) {
    let adapters = crate::network::get_adapters();
    for a in &adapters {
        if a.name == ethernet_name && a.status == "Up" {
            return ("有线".into(), ethernet_name.into());
        }
    }
    for a in &adapters {
        if a.name == wifi_name && a.status == "Up" {
            return ("无线".into(), wifi_name.into());
        }
    }
    ("未知".into(), "--".into())
}

pub fn run_ping_test(tx: mpsc::Sender<TestEvent>, gateway: String, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        if !stop.load(Ordering::Relaxed) { ping_host(&tx, &gateway, 80, &stop); }
        if !stop.load(Ordering::Relaxed) { ping_host(&tx, "www.baidu.com", 80, &stop); }
        if !stop.load(Ordering::Relaxed) { ping_host(&tx, "114.114.114.114", 53, &stop); }
        let _ = tx.send(TestEvent::PingDone);
    });
}

fn ping_host(tx: &mpsc::Sender<TestEvent>, host: &str, port: u16, stop: &Arc<AtomicBool>) {
    if stop.load(Ordering::Relaxed) { return; }
    use std::net::ToSocketAddrs;
    let addr = match format!("{}:{}", host, port).to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(a) => a,
            None => {
                let _ = tx.send(TestEvent::PingError { target: host.to_string(), error: "DNS resolve failed".into() });
                return;
            }
        },
        Err(e) => {
            let _ = tx.send(TestEvent::PingError { target: host.to_string(), error: e.to_string() });
            return;
        }
    };
    let start = Instant::now();
    match std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(3)) {
        Ok(_) => {
            let latency = start.elapsed().as_millis() as u64;
            let _ = tx.send(TestEvent::PingResult {
                target: host.to_string(),
                latency_ms: latency,
            });
        }
        Err(e) => {
            let _ = tx.send(TestEvent::PingError {
                target: host.to_string(),
                error: e.to_string(),
            });
        }
    }
}

pub fn run_speed_test(tx: mpsc::Sender<TestEvent>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let client = match reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(TestEvent::SpeedError(format!("HTTP client error: {}", e)));
                return;
            }
        };

        let start = Instant::now();
        let mut total_downloaded: u64 = 0;

        for url in TEST_URLS {
            if stop.load(Ordering::Relaxed) { break; }

            let resp = client.get(*url).send();
            let mut resp = match resp {
                Ok(r) if r.status().is_success() => r,
                _ => continue,
            };

            let mut buf = vec![0u8; 8192];
            loop {
                if stop.load(Ordering::Relaxed) { break; }
                use std::io::Read;
                match resp.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        total_downloaded += n as u64;
                        let elapsed_ms = start.elapsed().as_millis() as u64;
                        let speed_mbps = if elapsed_ms > 0 {
                            (total_downloaded as f64 * 8.0) / (elapsed_ms as f64 / 1000.0) / 1_000_000.0
                        } else {
                            0.0
                        };
                        let _ = tx.send(TestEvent::SpeedProgress {
                            downloaded: total_downloaded,
                            elapsed_ms,
                            speed_mbps,
                        });
                    }
                    Err(_) => break,
                }
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        let speed_mbps = if elapsed_ms > 0 {
            (total_downloaded as f64 * 8.0) / (elapsed_ms as f64 / 1000.0) / 1_000_000.0
        } else {
            0.0
        };
        let _ = tx.send(TestEvent::SpeedDone { speed_mbps, downloaded: total_downloaded, elapsed_ms });
    });
}
