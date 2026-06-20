use eframe::egui;

#[derive(Clone)]
pub struct Theme {
    pub name: &'static str,
    pub bg_dark: egui::Color32,
    pub bg_card: egui::Color32,
    pub bg_input: egui::Color32,
    pub accent: egui::Color32,
    pub text: egui::Color32,
    pub text_dim: egui::Color32,
    pub connected: egui::Color32,
    pub disconnected: egui::Color32,
    pub warning: egui::Color32,
    pub paused: egui::Color32,
    pub btn_blue: egui::Color32,
    pub btn_red: egui::Color32,
    pub separator: egui::Color32,
}

pub fn dark() -> Theme {
    Theme {
        name: "Dark",
        bg_dark: egui::Color32::from_rgb(18, 18, 30),
        bg_card: egui::Color32::from_rgb(25, 35, 60),
        bg_input: egui::Color32::from_rgb(18, 18, 30),
        accent: egui::Color32::from_rgb(0, 212, 170),
        text: egui::Color32::from_rgb(224, 224, 224),
        text_dim: egui::Color32::from_rgb(127, 140, 155),
        connected: egui::Color32::from_rgb(46, 204, 113),
        disconnected: egui::Color32::from_rgb(231, 76, 60),
        warning: egui::Color32::from_rgb(243, 156, 18),
        paused: egui::Color32::from_rgb(241, 196, 15),
        btn_blue: egui::Color32::from_rgb(52, 152, 219),
        btn_red: egui::Color32::from_rgb(192, 57, 43),
        separator: egui::Color32::from_rgb(50, 60, 90),
    }
}

pub fn light() -> Theme {
    Theme {
        name: "Light",
        bg_dark: egui::Color32::from_rgb(245, 247, 250),
        bg_card: egui::Color32::from_rgb(255, 255, 255),
        bg_input: egui::Color32::from_rgb(240, 242, 245),
        accent: egui::Color32::from_rgb(0, 150, 120),
        text: egui::Color32::from_rgb(33, 37, 41),
        text_dim: egui::Color32::from_rgb(108, 117, 125),
        connected: egui::Color32::from_rgb(40, 167, 69),
        disconnected: egui::Color32::from_rgb(220, 53, 69),
        warning: egui::Color32::from_rgb(255, 152, 0),
        paused: egui::Color32::from_rgb(133, 100, 4),
        btn_blue: egui::Color32::from_rgb(13, 110, 253),
        btn_red: egui::Color32::from_rgb(220, 53, 69),
        separator: egui::Color32::from_rgb(222, 226, 230),
    }
}

pub fn get_theme(name: &str) -> Theme {
    match name {
        "light" => light(),
        _ => dark(),
    }
}
