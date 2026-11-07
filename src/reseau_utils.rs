use std::net::UdpSocket;

pub fn adresse_ip_locale() -> String {
    if let Ok(s) = UdpSocket::bind("0.0.0.0:0") {
        if s.connect("8.8.8.8:80").is_ok() {
            if let Ok(local) = s.local_addr() {
                let ip = local.ip().to_string();
                if !ip.is_empty() && ip != "0.0.0.0" {
                    return ip;
                }
            }
        }
    }
    "127.0.0.1".into()
}
