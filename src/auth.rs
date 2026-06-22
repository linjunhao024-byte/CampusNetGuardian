use std::net::UdpSocket;

pub fn get_local_ip(gateway: &str) -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    let addr = format!("{}:801", gateway);
    socket.connect(&addr).ok()?;
    socket.local_addr().ok().map(|a| a.ip().to_string())
}

pub fn do_login(gateway: &str, student_id: &str, password: &str, ac_ip: &str) -> (bool, String) {
    let current_ip = match get_local_ip(gateway) {
        Some(ip) => ip,
        None => return (false, "无法获取本机IP".into()),
    };

    let login_url = format!("http://{}:801/eportal/", gateway);
    let params = [
        ("c", "Portal"),
        ("a", "login"),
        ("callback", "dr1004"),
        ("login_method", "1"),
        ("user_account", &format!(",0,{}", student_id)),
        ("user_password", password),
        ("wlan_user_ip", &current_ip),
        ("wlan_user_ipv6", ""),
        ("wlan_vlan_id", "0"),
        ("wlan_user_mac", "000000000000"),
        ("wlan_ac_ip", ac_ip),
        ("wlan_ac_name", ""),
        ("jsVersion", "3.3.3"),
    ];

    let resp = crate::network::get_http_client()
        .get(&login_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .query(&params)
        .send();

    match resp {
        Ok(r) => {
            let text = r.text().unwrap_or_default();
            if text.contains("\"result\":\"1\"") {
                (true, "认证通过。".into())
            } else {
                (false, format!("认证失败: {}", &text.chars().take(80).collect::<String>()))
            }
        }
        Err(e) => (false, format!("请求失败: {}", e)),
    }
}
