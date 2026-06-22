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
            .with_inner_size([820.0, 620.0])
            .with_min_inner_size([700.0, 520.0])
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
    log_lines: std::collections::VecDeque<String>,
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
    ping_rx: Option<mpsc::Receiver<crate::speedtest::TestEvent>>,
    speed_rx: Option<mpsc::Receiver<crate::speedtest::TestEvent>>,
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
            log_lines: std::collections::VecDeque::new(),
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
            ping_rx: None, speed_rx: None, net_type_rx: None, detect_rx,
            ping_stop: None, speed_stop: None,
            speed_testing: false, ping_testing: false,
            speed_mbps: 0.0, speed_downloaded: 0, speed_elapsed_ms: 0,
            speed_error: String::new(), ping_results: Vec::new(),
            net_type: String::new(), net_name: String::new(),
            start_time: Instant::now(),
            anim_phase: 0.0,
        }
    }

    fn apply_theme(&mut self) { self.theme = crate::theme::get_theme(&self.theme_name); }

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
        if let Some(h) = self.guardian_handle.take() { let _ = h.join(); }
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

    fn card(ui: &mut egui::Ui, t: &Theme, f: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::none().fill(t.bg_card).corner_radius(10.0)
            .inner_margin(egui::Margin::symmetric(18, 14))
            .stroke(egui::Stroke::new(1.0, t.separator))
            .show(ui, f);
    }

    fn btn(ui: &mut egui::Ui, t: &Theme, text: &str, color: egui::Color32) -> bool {
        ui.add(egui::Button::new(
            egui::RichText::new(text).size(13.0).color(egui::Color32::WHITE)
        ).fill(color).corner_radius(6))
        .clicked()
    }

    fn draw_config_form(&mut self, ui: &mut egui::Ui, compact: bool) {
        let t = self.theme.clone();
        ui.add_space(8.0);
        ui.heading(egui::RichText::new("认证信息").size(16.0).color(t.text));
        ui.add_space(6.0);
        Self::form_row(ui, "学号:", &mut self.config_student_id, false, &t);
        Self::form_row(ui, "密码:", &mut self.config_password, true, &t);
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(8.0);
        ui.heading(egui::RichText::new("网卡配置").size(16.0).color(t.text));
        ui.add_space(6.0);
        Self::form_row(ui, "有线网卡:", &mut self.config_ethernet, false, &t);
        Self::form_row(ui, "无线网卡:", &mut self.config_wifi, false, &t);
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(8.0);
        ui.heading(egui::RichText::new("网络配置").size(16.0).color(t.text));
        ui.add_space(6.0);
        Self::form_row(ui, "网关地址:", &mut self.config_gateway, false, &t);
        Self::form_row(ui, "AC_IP:", &mut self.config_ac_ip, false, &t);
        if !compact {
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);
            ui.heading(egui::RichText::new("潮汐参数").size(16.0).color(t.text));
            ui.add_space(6.0);
            Self::form_row(ui, "初始间隔(秒):", &mut self.config_base_interval, false, &t);
            Self::form_row(ui, "最大间隔(秒):", &mut self.config_max_interval, false, &t);
            Self::form_row(ui, "巡逻频率(秒):", &mut self.config_normal_interval, false, &t);
        }
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(8.0);
        ui.checkbox(&mut self.auto_start, egui::RichText::new("开机自动启动").size(14.0).color(t.text));
        ui.add_space(4.0);
        ui.checkbox(&mut self.force_ethernet, egui::RichText::new("有线网卡优先").size(14.0).color(t.text));
        if !compact {
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);
            ui.heading(egui::RichText::new("主题").size(16.0).color(t.text));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for (name, label) in [("dark", "暗色"), ("light", "亮色")] {
                    let selected = self.theme_name == name;
                    let color = if selected { t.accent } else { t.bg_input };
                    let text_c = if selected { egui::Color32::WHITE } else { t.text };
                    if ui.add(egui::Button::new(egui::RichText::new(label).size(13.0).color(text_c))
                        .fill(color).corner_radius(6)).clicked() {
                        self.theme_name = name.to_string();
                        self.apply_theme();
                    }
                }
            });
        }
    }

    fn form_row(ui: &mut egui::Ui, label: &str, value: &mut String, password: bool, t: &Theme) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).size(13.0).color(t.text_dim));
            let edit = egui::TextEdit::singleline(value).desired_width(300.0).font(egui::TextStyle::Body);
            ui.add(if password { edit.password(true) } else { edit });
        });
        ui.add_space(3.0);
    }

    fn status_icon(&self) -> (&'static str, String, egui::Color32) {
        let t = &self.theme;
        match &self.state {
            GuardianState::Initializing => ("○", String::from("初始化中"), t.text_dim),
            GuardianState::Connected { adapter } => ("●", format!("已连接 ({})", adapter), t.connected),
            GuardianState::Disconnected => ("●", String::from("链路中断"), t.disconnected),
            GuardianState::Retrying { interval, .. } => ("◌", format!("重试中 ({}s)", interval), t.warning),
            GuardianState::Phone { .. } => ("●", String::from("USB共享"), t.warning),
            GuardianState::Away => ("○", String::from("离线"), t.text_dim),
            GuardianState::Error => ("●", String::from("异常"), t.disconnected),
            GuardianState::Paused => ("◎", String::from("已关闭"), t.paused),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let t = self.theme.clone();

        ctx.style_mut(|s| {
            s.visuals.window_fill = t.bg_dark;
            s.visuals.panel_fill = t.bg_dark;
            s.visuals.widgets.noninteractive.bg_fill = t.bg_card;
            s.visuals.widgets.inactive.bg_fill = t.bg_input;
            s.visuals.widgets.hovered.bg_fill = t.accent;
            s.visuals.widgets.active.bg_fill = t.accent;
            s.visuals.selection.bg_fill = t.accent;
        });

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

        // 动画背景
        {
            let screen = ctx.screen_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("bg")));
            let (speed, color, amp) = match &self.state {
                GuardianState::Connected { .. } => (0.03, t.connected, 20.0),
                GuardianState::Retrying { .. } => (0.06, t.warning, 30.0),
                GuardianState::Disconnected | GuardianState::Error => (0.01, t.disconnected, 12.0),
                GuardianState::Phone { .. } => (0.04, t.warning, 24.0),
                GuardianState::Paused => (0.005, t.text_dim, 8.0),
                GuardianState::Away => (0.008, t.text_dim, 10.0),
                _ => (0.02, t.accent, 16.0),
            };
            self.anim_phase = (self.anim_phase + speed as f64) % 6.283185307;
            let phase = self.anim_phase as f32;
            let alpha_base: u8 = if t.name == "Light" { 25 } else { 12 };
            for i in 0..5 {
                let y_base = screen.top() + screen.height() * (0.15 + i as f32 * 0.18);
                let mut pts = Vec::new();
                let mut x = screen.left();
                while x <= screen.right() {
                    let p = phase + i as f32 * 0.7;
                    let y = y_base + (x * 0.006 + p).sin() * amp + (x * 0.012 + p * 1.4).sin() * amp * 0.4;
                    pts.push(egui::pos2(x, y));
                    x += 10.0;
                }
                let a = alpha_base.saturating_sub(i as u8 * 2);
                let c = if t.name == "Light" {
                    egui::Color32::from_rgba_unmultiplied(color.r()/2, color.g()/2, color.b()/2, a)
                } else {
                    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
                };
                painter.add(egui::Shape::line(pts, egui::Stroke::new(if t.name=="Light"{1.8}else{1.2}, c)));
            }
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
                self.net_type = ntype; self.net_name = nname; self.net_type_rx = None;
            }
        }
        if let Some(rx) = &self.log_rx {
            while let Ok(line) = rx.try_recv() {
                if self.log_lines.len() > 2000 { self.log_lines.pop_front(); }
                self.log_lines.push_back(line);
            }
        }
        if let Some(rx) = &self.state_rx {
            while let Ok(s) = rx.try_recv() {
                self.state_detail = match &s {
                    GuardianState::Connected { adapter } => format!("网卡: {}", adapter),
                    GuardianState::Retrying { adapter, interval, next_retry } =>
                        format!("{} | 退避 {}s | 下次 {:.1}s", adapter, interval, next_retry),
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
                    ui.add_space(50.0);
                    ui.label(egui::RichText::new("CampusNet Guardian").size(32.0).strong().color(t.accent));
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("广东培正学院 · 校园网自动认证引擎").size(14.0).color(t.text_dim));
                    ui.add_space(30.0);

                    Self::card(ui, &t, |ui| {
                        ui.label(egui::RichText::new("初次运行 — 配置向导").size(16.0).strong().color(t.text));
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("自动检测已完成，请确认或修改以下信息").size(13.0).color(t.text_dim));
                    });

                    ui.add_space(12.0);
                    egui::ScrollArea::vertical().show(ui, |ui| { self.draw_config_form(ui, true); });
                    ui.add_space(20.0);
                    if Self::btn(ui, &t, "    保存并启动    ", t.accent) {
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
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let (icon, text, color) = self.status_icon();
                ui.add_space(12.0);
                ui.label(egui::RichText::new(icon).size(28.0).color(color));
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(text).size(18.0).strong().color(color));
                    if !self.state_detail.is_empty() {
                        ui.label(egui::RichText::new(&self.state_detail).size(12.0).color(t.text_dim));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_paused = self.pause_flag.load(Ordering::Relaxed);
                    let (btn_text, btn_color) = if is_paused { ("  开启守护  ", t.connected) } else { ("  关闭守护  ", t.warning) };
                    if Self::btn(ui, &t, "  重启  ", t.btn_blue) { self.restart_guardian(); }
                    ui.add_space(6.0);
                    if Self::btn(ui, &t, btn_text, btn_color) {
                        if is_paused {
                            self.pause_flag.store(false, Ordering::Relaxed);
                            *self.disabled_adapter.lock().unwrap_or_else(|e| e.into_inner()) = None;
                        } else {
                            let mut disabled = self.disabled_adapter.lock().unwrap_or_else(|e| e.into_inner());
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
            ui.add_space(10.0);
        });

        // ── Tab 栏 ──
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                for (tab, label) in [(Tab::Log, "日志"), (Tab::Config, "配置"), (Tab::Status, "状态"), (Tab::SpeedTest, "测速")] {
                    let selected = self.tab == tab;
                    let (text_color, underline) = if selected {
                        (t.accent, Some(t.accent))
                    } else {
                        (t.text_dim, None)
                    };
                    ui.add_space(8.0);
                    let resp = ui.add(egui::Label::new(
                        egui::RichText::new(label).size(14.0).color(text_color)
                    ).sense(egui::Sense::click()));
                    if let Some(c) = underline {
                        let rect = resp.rect;
                        ui.painter().line_segment(
                            [egui::pos2(rect.left(), rect.bottom() + 2.0), egui::pos2(rect.right(), rect.bottom() + 2.0)],
                            egui::Stroke::new(2.0, c),
                        );
                    }
                    if resp.clicked() { self.tab = tab; }
                    ui.add_space(8.0);
                }
            });
            ui.add_space(6.0);
            ui.separator();
        });

        // ── 底部信息栏 ──
        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new("适用于广东培正学院").size(11.0).color(t.text_dim));
                ui.label(egui::RichText::new(" · ").size(11.0).color(t.separator));
                ui.label(egui::RichText::new("CampusNet Guardian V1.0.0").size(11.0).color(t.text_dim));
                ui.label(egui::RichText::new(" · ").size(11.0).color(t.separator));
                ui.hyperlink_to(egui::RichText::new("GitHub").size(11.0).color(t.accent),
                    "https://github.com/YOUR_USERNAME/CampusNetGuardian");
                ui.label(egui::RichText::new(" · ").size(11.0).color(t.separator));
                ui.hyperlink_to(egui::RichText::new("求Star").size(11.0).color(t.warning),
                    "https://github.com/YOUR_USERNAME/CampusNetGuardian");
            });
            ui.add_space(6.0);
        });

        // ── 主内容 ──
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(10.0);
            match self.tab {
                Tab::Log => {
                    if self.log_lines.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(100.0);
                            ui.label(egui::RichText::new("等待日志...").size(15.0).color(t.text_dim));
                        });
                    } else {
                        egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                            for (i, line) in self.log_lines.iter().enumerate() {
                                let bg = if i % 2 == 0 { egui::Color32::TRANSPARENT } else { t.bg_input };
                                egui::Frame::none().fill(bg).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                                    ui.label(egui::RichText::new(line).family(egui::FontFamily::Monospace).size(12.5).color(t.text));
                                });
                            }
                        });
                    }
                }
                Tab::Config => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.draw_config_form(ui, false);
                        ui.add_space(20.0);
                        if Self::btn(ui, &t, "  保存配置  ", t.accent) { self.save_config_from_ui(); }
                        ui.add_space(25.0);
                        Self::card(ui, &t, |ui| {
                            ui.label(egui::RichText::new("危险操作").size(14.0).strong().color(t.disconnected));
                            ui.add_space(8.0);
                            if Self::btn(ui, &t, "  一键卸载 (配置 + 日志 + 注册表)  ", t.btn_red) {
                                crate::config::uninstall_all();
                                self.stop_flag.store(true, Ordering::Relaxed);
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    });
                }
                Tab::Status => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let (si, st, sc) = self.status_icon();
                        Self::card(ui, &t, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(si).size(32.0).color(sc));
                                ui.add_space(14.0);
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(st).size(22.0).strong().color(sc));
                                    if !self.state_detail.is_empty() {
                                        ui.label(egui::RichText::new(&self.state_detail).size(13.0).color(t.text_dim));
                                    }
                                });
                            });
                        });
                        ui.add_space(10.0);

                        fn info_card(ui: &mut egui::Ui, title: &str, rows: &[(&str, &str)], t: &Theme) {
                            egui::Frame::none().fill(t.bg_card).corner_radius(10.0)
                                .inner_margin(egui::Margin::symmetric(16, 12))
                                .stroke(egui::Stroke::new(1.0, t.separator))
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new(title).size(14.0).strong().color(t.accent));
                                    ui.add_space(8.0);
                                    for (i, (k, v)) in rows.iter().enumerate() {
                                        if i > 0 { ui.add_space(4.0); }
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(*k).size(13.0).color(t.text_dim));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.label(egui::RichText::new(*v).size(13.0).strong().color(t.text));
                                            });
                                        });
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
                    let mut ping_done = false;
                    let mut speed_done = false;
                    if let Some(rx) = &self.ping_rx {
                        while let Ok(evt) = rx.try_recv() {
                            match evt {
                                crate::speedtest::TestEvent::PingResult { target, latency_ms } => { self.ping_results.push((target, format!("{} ms", latency_ms))); }
                                crate::speedtest::TestEvent::PingError { target, error } => { self.ping_results.push((target, format!("超时 ({})", error))); }
                                crate::speedtest::TestEvent::PingDone => { self.ping_testing = false; ping_done = true; }
                                _ => {}
                            }
                        }
                    }
                    if let Some(rx) = &self.speed_rx {
                        while let Ok(evt) = rx.try_recv() {
                            match evt {
                                crate::speedtest::TestEvent::SpeedProgress { downloaded, elapsed_ms, speed_mbps } => {
                                    self.speed_downloaded = downloaded; self.speed_elapsed_ms = elapsed_ms; self.speed_mbps = speed_mbps;
                                }
                                crate::speedtest::TestEvent::SpeedDone { speed_mbps, downloaded, elapsed_ms } => {
                                    self.speed_mbps = speed_mbps; self.speed_downloaded = downloaded; self.speed_elapsed_ms = elapsed_ms; self.speed_testing = false; speed_done = true;
                                }
                                crate::speedtest::TestEvent::SpeedError(e) => { self.speed_error = e; self.speed_testing = false; speed_done = true; }
                                _ => {}
                            }
                        }
                    }
                    if ping_done { self.ping_rx = None; }
                    if speed_done { self.speed_rx = None; }
                    if self.net_type.is_empty() && self.net_type_rx.is_none() {
                        let eth = self.config.ethernet_name.clone();
                        let wifi = self.config.wifi_name.clone();
                        let (tx, rx) = mpsc::channel();
                        self.net_type_rx = Some(rx);
                        std::thread::spawn(move || { let _ = tx.send(crate::speedtest::get_current_network_info(&eth, &wifi)); });
                    }

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        // 网络类型
                        Self::card(ui, &t, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("当前网络").size(13.0).color(t.text_dim));
                                ui.add_space(8.0);
                                let c = match self.net_type.as_str() {
                                    "有线" => t.connected, "无线" => t.btn_blue, _ => t.text_dim,
                                };
                                ui.label(egui::RichText::new(format!("{} ({})", self.net_type, self.net_name)).size(14.0).strong().color(c));
                            });
                        });

                        ui.add_space(12.0);

                        // 延迟测试
                        Self::card(ui, &t, |ui| {
                            ui.label(egui::RichText::new("延迟测试").size(15.0).strong().color(t.text));
                            ui.add_space(8.0);

                            egui::Grid::new("ping").striped(true).num_columns(2).spacing([50.0, 6.0]).show(ui, |ui| {
                                ui.label(egui::RichText::new("目标").size(13.0).strong().color(t.text_dim));
                                ui.label(egui::RichText::new("延迟").size(13.0).strong().color(t.text_dim));
                                ui.end_row();
                                for (target, latency) in &self.ping_results {
                                    ui.label(egui::RichText::new(target).size(13.0).color(t.text));
                                    let c = if latency.contains("ms") {
                                        let ms: u64 = latency.split_whitespace().next().unwrap_or("0").parse().unwrap_or(999);
                                        if ms < 50 { t.connected } else if ms < 150 { t.warning } else { t.disconnected }
                                    } else { t.disconnected };
                                    ui.label(egui::RichText::new(latency).size(13.0).strong().color(c));
                                    ui.end_row();
                                }
                            });

                            ui.add_space(8.0);
                            if self.ping_testing {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.add_space(6.0);
                                    ui.label(egui::RichText::new("测试中...").size(13.0).color(t.warning));
                                    ui.add_space(8.0);
                                    if Self::btn(ui, &t, "停止", t.btn_red) {
                                        if let Some(s) = &self.ping_stop { s.store(true, Ordering::Relaxed); }
                                        self.ping_testing = false;
                                    }
                                });
                            } else {
                                if Self::btn(ui, &t, "  测试延迟  ", t.btn_blue) {
                                    self.ping_testing = true;
                                    self.ping_results.clear();
                                    let stop = Arc::new(AtomicBool::new(false));
                                    self.ping_stop = Some(stop.clone());
                                    let (tx, rx) = mpsc::channel();
                                    self.ping_rx = Some(rx);
                                    crate::speedtest::run_ping_test(tx, self.config.gateway.clone(), stop);
                                }
                            }
                        });

                        ui.add_space(12.0);

                        // 下载测速
                        Self::card(ui, &t, |ui| {
                            ui.label(egui::RichText::new("下载测速").size(15.0).strong().color(t.text));
                            ui.add_space(10.0);

                            let speed_text = if self.speed_mbps > 0.0 { format!("{:.2}", self.speed_mbps) } else { "--".to_string() };
                            let speed_unit = if self.speed_mbps > 0.0 { " Mbps" } else { "" };
                            let speed_color = if self.speed_mbps > 50.0 { t.connected } else if self.speed_mbps > 10.0 { t.warning } else { t.text_dim };
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(speed_text).size(40.0).strong().color(speed_color));
                                ui.label(egui::RichText::new(speed_unit).size(16.0).color(t.text_dim));
                            });

                            if self.speed_downloaded > 0 {
                                ui.label(egui::RichText::new(format!("已下载 {:.1} MB · 耗时 {:.1}s",
                                    self.speed_downloaded as f64 / 1_000_000.0, self.speed_elapsed_ms as f64 / 1000.0))
                                    .size(12.0).color(t.text_dim));
                            }
                            if !self.speed_error.is_empty() {
                                ui.label(egui::RichText::new(&self.speed_error).size(13.0).color(t.disconnected));
                            }

                            ui.add_space(10.0);
                            if self.speed_testing {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.add_space(6.0);
                                    ui.label(egui::RichText::new("测速中...").size(13.0).color(t.warning));
                                    ui.add_space(8.0);
                                    if Self::btn(ui, &t, "停止", t.btn_red) {
                                        if let Some(s) = &self.speed_stop { s.store(true, Ordering::Relaxed); }
                                        self.speed_testing = false;
                                    }
                                });
                            } else {
                                if Self::btn(ui, &t, "  开始测速  ", t.btn_blue) {
                                    self.speed_testing = true;
                                    self.speed_mbps = 0.0; self.speed_downloaded = 0; self.speed_elapsed_ms = 0; self.speed_error.clear();
                                    let stop = Arc::new(AtomicBool::new(false));
                                    self.speed_stop = Some(stop.clone());
                                    let (tx, rx) = mpsc::channel();
                                    self.speed_rx = Some(rx);
                                    crate::speedtest::run_speed_test(tx, stop);
                                }
                            }
                        });
                    });
                }
            }
        });
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}
