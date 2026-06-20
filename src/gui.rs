use std::sync::{Arc, Mutex, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use eframe::egui;
use crate::config::{self, Config};
use crate::guardian::{GuardianThread, GuardianState};
use crate::tray;
use crate::theme::Theme;

fn load_chinese_font(ctx: &egui::Context) {
    for path in &["C:\\Windows\\Fonts\\msyh.ttc", "C:\\Windows\\Fonts\\simhei.ttf", "C:\\Windows\\Fonts\\simsun.ttc"] {
        if let Ok(data) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert("chinese".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(data)));
            if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Proportional) { f.push("chinese".to_owned()); }
            if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Monospace) { f.push("chinese".to_owned()); }
            ctx.set_fonts(fonts);
            return;
        }
    }
}

pub fn run_gui(dry_run: bool) -> eframe::Result {
    let title = if dry_run { "CampusNet Guardian - 广东培正学院 [测试版]" } else { "CampusNet Guardian - 广东培正学院" };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([680.0, 500.0])
            .with_title(title),
        ..Default::default()
    };
    eframe::run_native(title, options, Box::new(move |cc| {
        load_chinese_font(&cc.egui_ctx);
        Ok(Box::new(App::new(dry_run)))
    }))
}

struct App {
    config: Config,
    theme: Theme,
    log_lines: Vec<String>,
    state: GuardianState,
    state_detail: String,
    log_rx: Option<mpsc::Receiver<String>>,
    state_rx: Option<mpsc::Receiver<GuardianState>>,
    stop_flag: Arc<AtomicBool>,
    pause_flag: Arc<AtomicBool>,
    last_heartbeat: Arc<Mutex<Instant>>,
    disabled_adapter: Arc<Mutex<Option<String>>>,
    guardian_handle: Option<std::thread::JoinHandle<()>>,
    config_student_id: String,
    config_password: String,
    config_ethernet: String,
    config_wifi: String,
    config_gateway: String,
    config_ac_ip: String,
    config_base_interval: String,
    config_max_interval: String,
    config_normal_interval: String,
    force_ethernet: bool,
    auto_start: bool,
    theme_name: String,
    tab: Tab,
    show_wizard: bool,
    minimized: bool,
    tray_rx: Option<mpsc::Receiver<tray::TrayEvent>>,
    quit_flag: Arc<AtomicBool>,
    dry_run: bool,
    test_rx: Option<mpsc::Receiver<crate::speedtest::TestEvent>>,
    speed_testing: bool,
    ping_testing: bool,
    speed_mbps: f64,
    speed_downloaded: u64,
    speed_elapsed_ms: u64,
    speed_error: String,
    ping_results: Vec<(String, String)>,
    net_type: String,
    net_name: String,
    net_type_rx: Option<mpsc::Receiver<(String, String)>>,
    detect_rx: Option<mpsc::Receiver<(Option<String>, Option<String>, Option<String>, Option<String>)>>,
    ping_stop: Option<Arc<AtomicBool>>,
    speed_stop: Option<Arc<AtomicBool>>,
    start_time: Instant,
    anim_phase: f64,
}

#[derive(PartialEq)]
enum Tab { Log, Config, Status, SpeedTest }

impl App {
    fn new(dry_run: bool) -> Self {
        let cfg = config::load_config();
        let theme = crate::theme::get_theme(&cfg.theme);
        let config_exists = config::config_path().exists();
        let detect_rx = if !config_exists {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || { let _ = tx.send(crate::network::auto_detect_all()); });
            Some(rx)
        } else { None };
        let (tray_tx, tray_rx) = mpsc::channel();
        let quit_flag = Arc::new(AtomicBool::new(false));
        tray::create_tray(tray_tx, quit_flag.clone());
        Self {
            config_student_id: cfg.student_id.clone(),
            config_password: cfg.password.clone(),
            config_ethernet: cfg.ethernet_name.clone(),
            config_wifi: cfg.wifi_name.clone(),
            config_gateway: cfg.gateway.clone(),
            config_ac_ip: cfg.ac_ip.clone(),
            config_base_interval: cfg.base_retry_interval.to_string(),
            config_max_interval: cfg.max_retry_interval.to_string(),
            config_normal_interval: cfg.normal_check_interval.to_string(),
            force_ethernet: cfg.force_ethernet_priority,
            auto_start: cfg.auto_start,
            theme_name: cfg.theme.clone(),
            theme,
            config: cfg,
            log_lines: Vec::new(),
            state: GuardianState::Initializing,
            state_detail: String::new(),
            log_rx: None, state_rx: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
            pause_flag: Arc::new(AtomicBool::new(false)),
            last_heartbeat: Arc::new(Mutex::new(Instant::now())),
            disabled_adapter: Arc::new(Mutex::new(None)),
            guardian_handle: None,
            tab: Tab::Log,
            show_wizard: !config_exists,
            minimized: false,
            tray_rx: Some(tray_rx),
            quit_flag, dry_run,
            test_rx: None, net_type_rx: None, detect_rx,
            ping_stop: None, speed_stop: None,
            speed_testing: false, ping_testing: false,
            speed_mbps: 0.0, speed_downloaded: 0, speed_elapsed_ms: 0,
            speed_error: String::new(), ping_results: Vec::new(),
            net_type: String::new(), net_name: String::new(),
            start_time: Instant::now(),
            anim_phase: 0.0,
        }
    }

    fn apply_theme(&mut self) {
        self.theme = crate::theme::get_theme(&self.theme_name);
    }

    fn start_guardian(&mut self) {
        let (log_tx, log_rx) = mpsc::channel();
        let (state_tx, state_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let heartbeat = Arc::new(Mutex::new(Instant::now()));
        let disabled = Arc::new(Mutex::new(None));
        let config = Arc::new(Mutex::new(self.config.clone()));
        let gt = GuardianThread {
            config: config.clone(), log_tx, state_tx,
            stop: stop.clone(), pause: pause.clone(),
            last_heartbeat: heartbeat.clone(), disabled_adapter: disabled.clone(),
            dry_run: self.dry_run,
        };
        self.guardian_handle = Some(std::thread::spawn(move || gt.run()));
        self.log_rx = Some(log_rx);
        self.state_rx = Some(state_rx);
        self.stop_flag = stop;
        self.pause_flag = pause;
        self.last_heartbeat = heartbeat;
        self.disabled_adapter = disabled;
    }

    fn restart_guardian(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        self.pause_flag.store(false, Ordering::Relaxed);
        self.start_guardian();
    }

    fn save_config_from_ui(&mut self) {
        self.config.student_id = self.config_student_id.clone();
        self.config.password = self.config_password.clone();
        self.config.ethernet_name = self.config_ethernet.clone();
        self.config.wifi_name = self.config_wifi.clone();
        self.config.gateway = self.config_gateway.clone();
        self.config.ac_ip = self.config_ac_ip.clone();
        self.config.force_ethernet_priority = self.force_ethernet;
        self.config.auto_start = self.auto_start;
        self.config.theme = self.theme_name.clone();
        if let Ok(v) = self.config_base_interval.parse() { self.config.base_retry_interval = v; }
        if let Ok(v) = self.config_max_interval.parse() { self.config.max_retry_interval = v; }
        if let Ok(v) = self.config_normal_interval.parse() { self.config.normal_check_interval = v; }
        if self.dry_run { config::save_config_no_registry(&self.config); }
        else { config::save_config(&self.config); }
    }

    fn form_row(ui: &mut egui::Ui, label: &str, value: &mut String, password: bool, t: &Theme) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).size(15.0).color(t.text_dim));
            let edit = egui::TextEdit::singleline(value).desired_width(320.0).font(egui::TextStyle::Body);
            ui.add(if password { edit.password(true) } else { edit });
        });
        ui.add_space(4.0);
    }

    fn draw_config_form(&mut self, ui: &mut egui::Ui, compact: bool) {
        let t = self.theme.clone();
        ui.add_space(8.0);
        ui.heading(egui::RichText::new("认证信息").size(18.0).color(t.text));
        ui.add_space(6.0);
        Self::form_row(ui, "学号:", &mut self.config_student_id, false, &t);
        Self::form_row(ui, "密码:", &mut self.config_password, true, &t);

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.heading(egui::RichText::new("网卡配置").size(18.0).color(t.text));
        ui.add_space(6.0);
        Self::form_row(ui, "有线网卡:", &mut self.config_ethernet, false, &t);
        Self::form_row(ui, "无线网卡:", &mut self.config_wifi, false, &t);

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.heading(egui::RichText::new("网络配置").size(18.0).color(t.text));
        ui.add_space(6.0);
        Self::form_row(ui, "网关地址:", &mut self.config_gateway, false, &t);
        Self::form_row(ui, "AC_IP:", &mut self.config_ac_ip, false, &t);

        if !compact {
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            ui.heading(egui::RichText::new("潮汐参数").size(18.0).color(t.text));
            ui.add_space(6.0);
            Self::form_row(ui, "初始间隔(秒):", &mut self.config_base_interval, false, &t);
            Self::form_row(ui, "最大间隔(秒):", &mut self.config_max_interval, false, &t);
            Self::form_row(ui, "巡逻频率(秒):", &mut self.config_normal_interval, false, &t);
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.checkbox(&mut self.auto_start, egui::RichText::new("开机自动启动").size(15.0).color(t.text));
        ui.add_space(4.0);
        ui.checkbox(&mut self.force_ethernet, egui::RichText::new("有线网卡优先").size(15.0).color(t.text));

        if !compact {
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            ui.heading(egui::RichText::new("主题").size(18.0).color(t.text));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for (name, label) in [("dark", "暗色"), ("light", "亮色")] {
                    let selected = self.theme_name == name;
                    let btn = egui::Button::new(egui::RichText::new(label).size(14.0).color(
                        if selected { egui::Color32::WHITE } else { t.text }
                    )).fill(if selected { t.accent } else { t.bg_input });
                    if ui.add(btn).clicked() {
                        self.theme_name = name.to_string();
                        self.apply_theme();
                    }
                }
            });
        }
    }

    fn status_icon(&self) -> (&'static str, String, egui::Color32) {
        let t = &self.theme;
        match &self.state {
            GuardianState::Initializing => ("[ ]", String::from("初始化中"), t.text_dim),
            GuardianState::Connected { adapter } => ("[V]", format!("已连接 ({})", adapter), t.connected),
            GuardianState::Disconnected => ("[X]", String::from("链路中断"), t.disconnected),
            GuardianState::Retrying { interval, .. } => ("[~]", format!("重试中 ({}s)", interval), t.warning),
            GuardianState::Phone { .. } => ("[U]", String::from("USB共享"), t.warning),
            GuardianState::Away => ("[ ]", String::from("离线"), t.text_dim),
            GuardianState::Error => ("[!]", String::from("异常"), t.disconnected),
            GuardianState::Paused => ("[=]", String::from("已关闭"), t.paused),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let t = self.theme.clone();

        // 全局背景色 + 动画
        self.anim_phase += 0.02;
        ctx.style_mut(|s| {
            s.visuals.window_fill = t.bg_dark;
            s.visuals.panel_fill = t.bg_dark;
        });

        // 动画背景 - 网络状态波浪
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("animated_bg"),
        ));
        let t_anim = self.anim_phase as f32;
        let (wave_speed, wave_color, wave_amp) = match &self.state {
            GuardianState::Connected { .. } => (0.03, t.connected, 20.0),
            GuardianState::Retrying { .. } => (0.06, t.warning, 30.0),
            GuardianState::Disconnected | GuardianState::Error => (0.01, t.disconnected, 12.0),
            GuardianState::Phone { .. } => (0.04, t.warning, 24.0),
            GuardianState::Paused => (0.005, t.text_dim, 8.0),
            GuardianState::Away => (0.008, t.text_dim, 10.0),
            _ => (0.02, t.accent, 16.0),
        };
        self.anim_phase += wave_speed as f64;
        let alpha_base: u8 = if t.name == "Light" { 30 } else { 15 };
        for line_i in 0..4 {
            let y_base = screen.top() + screen.height() * (0.2 + line_i as f32 * 0.2);
            let mut points = Vec::new();
            let step = 8.0;
            let mut x = screen.left();
            while x <= screen.right() {
                let phase = t_anim + line_i as f32 * 0.8;
                let y = y_base + (x * 0.008 + phase).sin() * wave_amp
                    + (x * 0.015 + phase * 1.3).sin() * wave_amp * 0.5;
                points.push(egui::pos2(x, y));
                x += step;
            }
            let alpha = alpha_base.saturating_sub(line_i as u8 * 2);
            let c = if t.name == "Light" {
                egui::Color32::from_rgba_unmultiplied(
                    wave_color.r() / 2, wave_color.g() / 2, wave_color.b() / 2, alpha,
                )
            } else {
                egui::Color32::from_rgba_unmultiplied(
                    wave_color.r(), wave_color.g(), wave_color.b(), alpha,
                )
            };
            let stroke = if t.name == "Light" { 2.0 } else { 1.5 };
            painter.add(egui::Shape::line(points, egui::Stroke::new(stroke, c)));
        }

        if let Some(rx) = &self.tray_rx {
            while let Ok(evt) = rx.try_recv() {
                match evt {
                    tray::TrayEvent::Show => {
                        self.minimized = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    tray::TrayEvent::Quit => {
                        self.stop_flag.store(true, Ordering::Relaxed);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }
        if self.quit_flag.load(Ordering::Relaxed) {
            self.stop_flag.store(true, Ordering::Relaxed);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.minimized {
            ctx.request_repaint_after(Duration::from_millis(500));
            return;
        }
        if let Some(rx) = &self.detect_rx {
            if let Ok((eth, wifi, gw, ac)) = rx.try_recv() {
                if let Some(e) = eth { self.config_ethernet = e; }
                if let Some(w) = wifi { self.config_wifi = w; }
                if let Some(g) = gw { self.config_gateway = g; }
                if let Some(a) = ac { self.config_ac_ip = a; }
                self.detect_rx = None;
            }
        }
        if let Some(rx) = &self.net_type_rx {
            if let Ok((ntype, nname)) = rx.try_recv() {
                self.net_type = ntype;
                self.net_name = nname;
                self.net_type_rx = None;
            }
        }
        if let Some(rx) = &self.log_rx {
            while let Ok(line) = rx.try_recv() {
                self.log_lines.push(line);
                if self.log_lines.len() > 2000 { self.log_lines.remove(0); }
            }
        }
        if let Some(rx) = &self.state_rx {
            while let Ok(s) = rx.try_recv() {
                self.state_detail = match &s {
                    GuardianState::Connected { adapter } => format!("网卡: {}", adapter),
                    GuardianState::Retrying { adapter, interval, next_retry } =>
                        format!("网卡: {} | 退避: {}s | 下次: {:.1}s", adapter, interval, next_retry),
                    GuardianState::Phone { adapter } => format!("USB: {}", adapter),
                    _ => String::new(),
                };
                self.state = s;
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.minimized = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // ── 配置向导 ──
        if self.show_wizard {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.heading(egui::RichText::new("CampusNet Guardian").size(28.0).color(t.accent));
                    ui.label(egui::RichText::new("广东培正学院 | 校园网自动认证引擎").size(16.0).color(t.text_dim));
                    ui.label(egui::RichText::new("初次运行 - 配置向导").size(14.0).color(t.text_dim));
                    ui.add_space(20.0);
                    ui.label(egui::RichText::new("请填写以下配置信息，也可稍后在「配置」标签页中修改。").size(15.0).color(t.text));
                    ui.add_space(10.0);
                    egui::ScrollArea::vertical().show(ui, |ui| { self.draw_config_form(ui, true); });
                    ui.add_space(20.0);
                    if ui.add(egui::Button::new(egui::RichText::new("    保存并启动    ").size(16.0).color(egui::Color32::WHITE))
                        .fill(t.accent)).clicked() {
                        self.save_config_from_ui();
                        self.restart_guardian();
                        self.show_wizard = false;
                    }
                });
            });
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }

        // ── 状态栏 ──
        egui::TopBottomPanel::top("status_bar").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let (icon, text, color) = self.status_icon();
                ui.label(egui::RichText::new(icon).size(24.0).family(egui::FontFamily::Monospace).color(color));
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(text).size(20.0).strong().color(color));
                    if !self.state_detail.is_empty() {
                        ui.label(egui::RichText::new(&self.state_detail).size(13.0).color(t.text_dim));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_paused = self.pause_flag.load(Ordering::Relaxed);
                    let btn_text = if is_paused { "  开启守护  " } else { "  关闭守护  " };
                    let btn_color = if is_paused { t.connected } else { t.warning };
                    if ui.add(egui::Button::new(egui::RichText::new("  重启  ").size(14.0).color(egui::Color32::WHITE))
                        .fill(t.btn_blue)).clicked() { self.restart_guardian(); }
                    ui.add_space(6.0);
                    if ui.add(egui::Button::new(egui::RichText::new(btn_text).size(14.0).color(egui::Color32::WHITE))
                        .fill(btn_color)).clicked() {
                        if is_paused {
                            self.pause_flag.store(false, Ordering::Relaxed);
                            *self.disabled_adapter.lock().unwrap() = None;
                        } else {
                            let mut disabled = self.disabled_adapter.lock().unwrap();
                            if let Some(ref name) = *disabled {
                                if name != "__none__" && !crate::network::check_internet() {
                                    crate::network::set_adapter(name, true);
                                }
                            }
                            *disabled = None;
                            self.pause_flag.store(true, Ordering::Relaxed);
                        }
                    }
                });
            });
            ui.add_space(6.0);
        });

        // ── Tab 栏 ──
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for (tab, label) in [(Tab::Log, "  日志  "), (Tab::Config, "  配置  "), (Tab::Status, "  状态  "), (Tab::SpeedTest, "  测速  ")] {
                    let selected = self.tab == tab;
                    let btn = egui::Button::new(egui::RichText::new(label).size(15.0).color(
                        if selected { egui::Color32::WHITE } else { t.text }
                    )).fill(if selected { t.accent } else { egui::Color32::TRANSPARENT });
                    if ui.add(btn).clicked() { self.tab = tab; }
                }
            });
            ui.add_space(4.0);
        });

        // ── 底部信息栏 ──
        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new("适用于广东培正学院").size(11.0).color(t.text_dim));
                ui.label(egui::RichText::new(" | ").size(11.0).color(t.separator));
                ui.label(egui::RichText::new("CampusNet Guardian V1.0.0").size(11.0).color(t.text_dim));
                ui.label(egui::RichText::new(" | ").size(11.0).color(t.separator));
                ui.hyperlink_to(
                    egui::RichText::new("GitHub").size(11.0).color(t.accent),
                    "https://github.com/YOUR_USERNAME/CampusNetGuardian"
                );
                ui.label(egui::RichText::new(" | ").size(11.0).color(t.separator));
                ui.hyperlink_to(
                    egui::RichText::new("求Star").size(11.0).color(t.warning),
                    "https://github.com/YOUR_USERNAME/CampusNetGuardian"
                );
            });
        });

        // ── 主内容 ──
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            match self.tab {
                Tab::Log => {
                    if self.log_lines.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(80.0);
                            ui.label(egui::RichText::new("等待日志...").size(16.0).color(t.text_dim));
                        });
                    } else {
                        egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                            for line in &self.log_lines {
                                ui.label(egui::RichText::new(line).family(egui::FontFamily::Monospace).size(13.0).color(t.text));
                            }
                        });
                    }
                }
                Tab::Config => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.draw_config_form(ui, false);
                        ui.add_space(20.0);
                        if ui.add(egui::Button::new(egui::RichText::new("  保存配置  ").size(15.0).color(egui::Color32::WHITE))
                            .fill(t.accent)).clicked() { self.save_config_from_ui(); }
                        ui.add_space(25.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("危险操作").size(15.0).color(t.disconnected));
                        ui.add_space(6.0);
                        if ui.add(egui::Button::new(egui::RichText::new("  一键卸载 (删除配置、日志、注册表)  ").size(14.0).color(egui::Color32::WHITE))
                            .fill(t.btn_red)).clicked() {
                            crate::config::uninstall_all();
                            self.stop_flag.store(true, Ordering::Relaxed);
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                }
                Tab::Status => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let (si, st, sc) = self.status_icon();

                        egui::Frame::none().fill(t.bg_card).corner_radius(8.0)
                            .inner_margin(egui::Margin::symmetric(20, 16)).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(si).size(28.0).family(egui::FontFamily::Monospace).color(sc));
                                    ui.add_space(12.0);
                                    ui.vertical(|ui| {
                                        ui.label(egui::RichText::new(st).size(22.0).strong().color(sc));
                                        if !self.state_detail.is_empty() {
                                            ui.label(egui::RichText::new(&self.state_detail).size(14.0).color(t.text_dim));
                                        }
                                    });
                                });
                            });

                        ui.add_space(10.0);

                        fn info_card(ui: &mut egui::Ui, title: &str, rows: &[(&str, &str)], t: &Theme) {
                            egui::Frame::none().fill(t.bg_card).corner_radius(8.0)
                                .inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
                                    ui.label(egui::RichText::new(title).size(15.0).strong().color(t.accent));
                                    ui.add_space(6.0);
                                    for (k, v) in rows {
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(*k).size(14.0).color(t.text_dim));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.label(egui::RichText::new(*v).size(14.0).color(t.text));
                                            });
                                        });
                                        ui.add_space(2.0);
                                    }
                                });
                        }

                        info_card(ui, "网络", &[
                            ("网关地址", &self.config.gateway),
                            ("AC_IP", &self.config.ac_ip),
                            ("有线网卡", &self.config.ethernet_name),
                            ("无线网卡", &self.config.wifi_name),
                        ], &t);
                        ui.add_space(8.0);
                        info_card(ui, "认证", &[("学号", &self.config.student_id)], &t);
                        ui.add_space(8.0);
                        info_card(ui, "参数", &[
                            ("初始间隔", &format!("{}s", self.config.base_retry_interval)),
                            ("最大间隔", &format!("{}s", self.config.max_retry_interval)),
                            ("巡逻频率", &format!("{}s", self.config.normal_check_interval)),
                            ("自动启动", if self.config.auto_start { "是" } else { "否" }),
                            ("有线优先", if self.config.force_ethernet_priority { "是" } else { "否" }),
                            ("主题", if self.theme_name == "light" { "亮色" } else { "暗色" }),
                        ], &t);
                    });
                }
                Tab::SpeedTest => {
                    if let Some(rx) = &self.test_rx {
                        while let Ok(evt) = rx.try_recv() {
                            match evt {
                                crate::speedtest::TestEvent::PingResult { target, latency_ms } => { self.ping_results.push((target, format!("{} ms", latency_ms))); }
                                crate::speedtest::TestEvent::PingError { target, error } => { self.ping_results.push((target, format!("超时 ({})", error))); }
                                crate::speedtest::TestEvent::PingDone => { self.ping_testing = false; }
                                crate::speedtest::TestEvent::SpeedProgress { downloaded, elapsed_ms, speed_mbps } => {
                                    self.speed_downloaded = downloaded; self.speed_elapsed_ms = elapsed_ms; self.speed_mbps = speed_mbps;
                                }
                                crate::speedtest::TestEvent::SpeedDone { speed_mbps, downloaded, elapsed_ms } => {
                                    self.speed_mbps = speed_mbps; self.speed_downloaded = downloaded; self.speed_elapsed_ms = elapsed_ms; self.speed_testing = false;
                                }
                                crate::speedtest::TestEvent::SpeedError(e) => { self.speed_error = e; self.speed_testing = false; }
                            }
                        }
                    }
                    if self.net_type.is_empty() && self.net_type_rx.is_none() {
                        let eth = self.config.ethernet_name.clone();
                        let wifi = self.config.wifi_name.clone();
                        let (tx, rx) = mpsc::channel();
                        self.net_type_rx = Some(rx);
                        std::thread::spawn(move || { let _ = tx.send(crate::speedtest::get_current_network_info(&eth, &wifi)); });
                    }

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("当前网络:").size(15.0).color(t.text_dim));
                            ui.add_space(8.0);
                            let net_color = match self.net_type.as_str() {
                                "有线" => t.connected,
                                "无线" => t.btn_blue,
                                _ => t.text_dim,
                            };
                            ui.label(egui::RichText::new(format!("{} ({})", self.net_type, self.net_name)).size(15.0).strong().color(net_color));
                        });

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(12.0);

                        ui.heading(egui::RichText::new("延迟测试").size(18.0).color(t.text));
                        ui.add_space(8.0);

                        egui::Grid::new("ping_grid").striped(true).num_columns(2).spacing([40.0, 8.0]).show(ui, |ui| {
                            ui.label(egui::RichText::new("目标").size(15.0).strong().color(t.text));
                            ui.label(egui::RichText::new("延迟").size(15.0).strong().color(t.text));
                            ui.end_row();
                            for (target, latency) in &self.ping_results {
                                ui.label(egui::RichText::new(target).size(14.0).color(t.text));
                                let color = if latency.contains("ms") {
                                    let ms: u64 = latency.split_whitespace().next().unwrap_or("0").parse().unwrap_or(999);
                                    if ms < 50 { t.connected } else if ms < 150 { t.warning } else { t.disconnected }
                                } else { t.disconnected };
                                ui.label(egui::RichText::new(latency).size(14.0).color(color));
                                ui.end_row();
                            }
                        });

                        ui.add_space(8.0);
                        if self.ping_testing {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("正在测试...").size(14.0).color(t.warning));
                                ui.add_space(8.0);
                                if ui.add(egui::Button::new(egui::RichText::new("  停止  ").size(14.0).color(egui::Color32::WHITE))
                                    .fill(t.btn_red)).clicked() {
                                    if let Some(s) = &self.ping_stop { s.store(true, Ordering::Relaxed); }
                                    self.ping_testing = false;
                                }
                            });
                        } else {
                            if ui.add(egui::Button::new(egui::RichText::new("  测试延迟  ").size(14.0).color(egui::Color32::WHITE))
                                .fill(t.btn_blue)).clicked() {
                                self.ping_testing = true;
                                self.ping_results.clear();
                                let stop = Arc::new(AtomicBool::new(false));
                                self.ping_stop = Some(stop.clone());
                                let (tx, rx) = mpsc::channel();
                                self.test_rx = Some(rx);
                                crate::speedtest::run_ping_test(tx, self.config.gateway.clone(), stop);
                            }
                        }

                        ui.add_space(20.0);
                        ui.separator();
                        ui.add_space(12.0);

                        ui.heading(egui::RichText::new("下载测速").size(18.0).color(t.text));
                        ui.add_space(8.0);

                        if self.speed_testing {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("测速中...").size(14.0).color(t.warning));
                                ui.add_space(8.0);
                                if ui.add(egui::Button::new(egui::RichText::new("  停止  ").size(14.0).color(egui::Color32::WHITE))
                                    .fill(t.btn_red)).clicked() {
                                    if let Some(s) = &self.speed_stop { s.store(true, Ordering::Relaxed); }
                                    self.speed_testing = false;
                                }
                            });
                        }

                        let speed_text = if self.speed_mbps > 0.0 { format!("{:.2} Mbps", self.speed_mbps) } else { "--".to_string() };
                        let speed_color = if self.speed_mbps > 50.0 { t.connected } else if self.speed_mbps > 10.0 { t.warning } else { t.text_dim };
                        ui.label(egui::RichText::new(&speed_text).size(36.0).strong().color(speed_color));

                        if self.speed_downloaded > 0 {
                            ui.label(egui::RichText::new(format!("已下载: {:.1} MB | 耗时: {:.1}s",
                                self.speed_downloaded as f64 / 1_000_000.0, self.speed_elapsed_ms as f64 / 1000.0)).size(14.0).color(t.text));
                        }
                        if !self.speed_error.is_empty() {
                            ui.label(egui::RichText::new(&self.speed_error).size(14.0).color(t.disconnected));
                        }

                        ui.add_space(8.0);
                        if !self.speed_testing {
                            if ui.add(egui::Button::new(egui::RichText::new("  开始测速  ").size(14.0).color(egui::Color32::WHITE))
                                .fill(t.btn_blue)).clicked() {
                                self.speed_testing = true;
                                self.speed_mbps = 0.0; self.speed_downloaded = 0; self.speed_elapsed_ms = 0; self.speed_error.clear();
                                let stop = Arc::new(AtomicBool::new(false));
                                self.speed_stop = Some(stop.clone());
                                let (tx, rx) = mpsc::channel();
                                self.test_rx = Some(rx);
                                crate::speedtest::run_speed_test(tx, stop);
                            }
                        }
                    });
                }
            }
        });
        ctx.request_repaint();
    }
}
