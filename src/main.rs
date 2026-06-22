#![windows_subsystem = "windows"]

mod config;
mod network;
mod auth;
mod guardian;
mod gui;
mod cli;
mod tray;
mod speedtest;
mod theme;

const INSTANCE_PORT: u16 = 65432;

fn acquire_single_instance() -> Option<std::net::TcpListener> {
    std::net::TcpListener::bind(("127.0.0.1", INSTANCE_PORT)).ok()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let is_test = args.iter().any(|a| a == "--test" || a == "--test-gui");

    let _lock = if !is_test {
        match acquire_single_instance() {
            Some(l) => Some(l),
            None => {
                unsafe {
                    windows_sys::Win32::System::Console::AllocConsole();
                    eprintln!("CampusNet Guardian 已在运行中，请勿重复启动。");
                }
                std::thread::sleep(std::time::Duration::from_secs(3));
                std::process::exit(0);
            }
        }
    } else {
        None
    };

    if args.iter().any(|a| a == "--test") {
        run_test();
    } else if args.iter().any(|a| a == "--test-gui") {
        if let Err(e) = gui::run_gui(true) {
            eprintln!("GUI 启动失败: {}", e);
            std::process::exit(1);
        }
    } else if args.iter().any(|a| a == "--cli" || a == "-c") {
        cli::run_cli();
    } else {
        if let Err(e) = gui::run_gui(false) {
            eprintln!("GUI 启动失败: {}", e);
            std::process::exit(1);
        }
    }
}

fn run_test() {
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(0xFFFFFFFF); // ATTACH_PARENT_PROCESS
    }
    println!("=== CampusNet Guardian 网络检测（只读模式） ===\n");

    let adapters = network::get_adapters();
    println!("[1] 网卡检测");
    println!("  {:<20} {:<55} {}", "名称", "描述", "状态");
    println!("  {:<20} {:<55} {}", "----", "----", "----");
    for a in &adapters {
        println!("  {:<20} {:<55} {}", a.name, a.description, a.status);
    }

    println!("\n[2] 网卡自动识别");
    if let Some(ref e) = network::detect_ethernet_name(&adapters) {
        println!("  有线网卡: {}", e);
    } else {
        println!("  有线网卡: 未检测到");
    }
    if let Some(ref w) = network::detect_wifi_name(&adapters) {
        println!("  无线网卡: {}", w);
    } else {
        println!("  无线网卡: 未检测到");
    }

    println!("\n[3] 网关检测");
    match network::detect_gateway() {
        Some(ref gw) => println!("  网关: {}", gw),
        None => println!("  网关: 未检测到"),
    }

    println!("\n[4] AC_IP 检测");
    match network::detect_gateway().and_then(|gw| network::detect_ac_ip(&gw)) {
        Some(ref ac) => println!("  AC_IP: {}", ac),
        None => println!("  AC_IP: 未检测到"),
    }

    println!("\n[5] 外网连通性");
    if network::check_internet() {
        println!("  百度: 可达（有外网）");
    } else {
        println!("  百度: 不可达（无外网）");
    }

    println!("\n[6] 网关可达性");
    if let Some(ref gw) = network::detect_gateway() {
        if network::is_at_school(gw) {
            println!("  {} :801 可达（在校园网环境）", gw);
        } else {
            println!("  {} :801 不可达（不在校园网或网关未响应）", gw);
        }
    }

    println!("\n[7] USB 共享设备检测");
    let campus = network::detect_ethernet_name(&adapters)
        .into_iter()
        .chain(network::detect_wifi_name(&adapters))
        .collect::<Vec<_>>();
    let campus_refs: Vec<&str> = campus.iter().map(|s| s.as_str()).collect();
    let usb_devices: Vec<_> = adapters.iter().filter(|a| {
        a.status == "Up" && !campus_refs.contains(&a.name.as_str())
    }).collect();
    if usb_devices.is_empty() {
        println!("  无");
    } else {
        for d in &usb_devices {
            println!("  {} ({})", d.name, d.description);
        }
    }

    println!("\n[8] 虚拟网卡排除");
    let virtuals: Vec<_> = adapters.iter().filter(|a| {
        let n = a.name.to_lowercase();
        let d = a.description.to_lowercase();
        network::VIRTUAL_KEYWORDS.iter().any(|k| n.contains(k) || d.contains(k))
    }).collect();
    if virtuals.is_empty() {
        println!("  无");
    } else {
        for v in &virtuals {
            println!("  {} ({}) — 已排除", v.name, v.description);
        }
    }

    println!("\n=== 检测完成（未修改任何内容） ===");
    println!("按回车退出...");
    let mut input = String::new();
    let _ = std::io::stdin().read_line(&mut input);
}
