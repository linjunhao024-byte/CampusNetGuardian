use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use crate::config::{self, Config};
use crate::guardian::{GuardianThread, GuardianState};
use crate::network;

pub fn run_cli() {
    let mut cfg = config::load_config();
    let config_path = config::config_path();

    if !config_path.exists() {
        println!("=== CampusNet Guardian 首次配置向导 ===");
        println!();

        let (eth, wifi, gw, ac) = network::auto_detect_all();
        if let Some(ref e) = eth { println!("  检测到有线网卡: {}", e); cfg.ethernet_name = e.clone(); }
        if let Some(ref w) = wifi { println!("  检测到无线网卡: {}", w); cfg.wifi_name = w.clone(); }
        if let Some(ref g) = gw { println!("  检测到网关: {}", g); cfg.gateway = g.clone(); }
        if let Some(ref a) = ac { println!("  检测到 AC_IP: {}", a); cfg.ac_ip = a.clone(); }
        println!();

        println!("请输入校园网认证信息：");
        let stdin = std::io::stdin();
        let mut input = String::new();

        print!("  学号: ");
        std::io::Write::flush(&mut std::io::stdout()).ok();
        input.clear();
        stdin.read_line(&mut input).ok();
        let sid = input.trim().to_string();
        if !sid.is_empty() { cfg.student_id = sid; }

        print!("  密码: ");
        std::io::Write::flush(&mut std::io::stdout()).ok();
        input.clear();
        stdin.read_line(&mut input).ok();
        let pwd = input.trim().to_string();
        if !pwd.is_empty() { cfg.password = pwd; }

        print!("  开机自启？(y/n): ");
        std::io::Write::flush(&mut std::io::stdout()).ok();
        input.clear();
        stdin.read_line(&mut input).ok();
        cfg.auto_start = input.trim().to_lowercase() == "y";

        config::save_config(&cfg);
        println!();
        println!("配置已保存到 config.json");
        println!();
    }

    println!("[命令] stop=关闭守护 | start=开启守护 | restart=重启 | quit=退出");

    let (log_tx, log_rx) = mpsc::channel();
    let (state_tx, _state_rx) = mpsc::channel();

    let stop = Arc::new(AtomicBool::new(false));
    let pause = Arc::new(AtomicBool::new(false));
    let heartbeat = Arc::new(Mutex::new(Instant::now()));
    let disabled = Arc::new(Mutex::new(None));
    let config = Arc::new(Mutex::new(cfg.clone()));

    let gt = GuardianThread {
        config: config.clone(),
        log_tx,
        state_tx,
        stop: stop.clone(),
        pause: pause.clone(),
        last_heartbeat: heartbeat.clone(),
        disabled_adapter: disabled.clone(),
        dry_run: false,
    };

    let _guardian_handle = std::thread::spawn(move || gt.run());

    // 日志打印线程
    let _log_handle = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if let Ok(line) = log_rx.recv() {
                    println!("{}", line);
                }
            }
        })
    };

    // 看门狗
    let _watchdog = {
        let stop = stop.clone();
        let pause = pause.clone();
        let heartbeat = heartbeat.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(20));
                if pause.load(Ordering::Relaxed) { continue; }
                let elapsed = heartbeat.lock().unwrap().elapsed().as_secs();
                if elapsed > 25 {
                    println!("[!] 主循环疑似卡死 ({}s 无响应)，强制重启进程...", elapsed);
                    std::process::exit(1);
                }
            }
        })
    };

    // 命令输入
    let stdin = std::io::stdin();
    let mut input = String::new();
    loop {
        input.clear();
        if stdin.read_line(&mut input).is_err() { break; }
        let cmd = input.trim().to_lowercase();

        match cmd.as_str() {
            "stop" | "s" => {
                if !pause.load(Ordering::Relaxed) {
                    let mut disabled = disabled.lock().unwrap();
                    if let Some(ref name) = *disabled {
                        if name != "__none__" {
                            if !network::check_internet() {
                                network::set_adapter(name, true);
                            }
                        }
                    }
                    *disabled = None;
                    pause.store(true, Ordering::Relaxed);
                    println!("[*] 守护已关闭");
                }
            }
            "start" | "r" => {
                if pause.load(Ordering::Relaxed) {
                    *disabled.lock().unwrap() = None;
                    pause.store(false, Ordering::Relaxed);
                    println!("[*] 守护已开启");
                }
            }
            "restart" => {
                *disabled.lock().unwrap() = None;
                stop.store(true, Ordering::Relaxed);
                println!("[*] 守护已重启");
                let exe = std::env::current_exe().unwrap();
                let _ = std::process::Command::new(exe).arg("--cli").spawn();
                std::process::exit(0);
            }
            "quit" | "q" => {
                stop.store(true, Ordering::Relaxed);
                println!("[*] 退出");
                break;
            }
            _ => {
                println!("[命令] stop=关闭守护 | start=开启守护 | restart=重启 | quit=退出");
            }
        }
    }
}
