use std::net::UdpSocket;

pub fn adresse_ip_locale() -> String {
    let mut candidats: Vec<String> = Vec::new();

    for cible in ["8.8.8.8:80", "1.1.1.1:80", "192.168.1.1:80"] {
        if let Ok(s) = UdpSocket::bind("0.0.0.0:0") {
            if s.connect(cible).is_ok() {
                if let Ok(local) = s.local_addr() {
                    let ip = local.ip().to_string();
                    if est_ip_lan(&ip) && !candidats.contains(&ip) {
                        candidats.push(ip);
                    }
                }
            }
        }
    }

    if let Some(premier) = candidats.first() {
        return premier.clone();
    }

    if let Ok(sortie) = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -notlike '169.254.*' -and $_.IPAddress -ne '127.0.0.1' } | Select-Object -ExpandProperty IPAddress",
        ])
        .output()
    {
        if sortie.status.success() {
            let texte = String::from_utf8_lossy(&sortie.stdout);
            for ligne in texte.lines() {
                let ip = ligne.trim().to_string();
                if est_ip_lan(&ip) {
                    return ip;
                }
            }
        }
    }

    "127.0.0.1".into()
}

fn est_ip_lan(ip: &str) -> bool {
    if ip.is_empty() || ip == "0.0.0.0" || ip == "127.0.0.1" {
        return false;
    }
    let parties: Vec<&str> = ip.split('.').collect();
    if parties.len() != 4 {
        return false;
    }
    if parties[0] == "169" && parties[1] == "254" {
        return false;
    }
    true
}

