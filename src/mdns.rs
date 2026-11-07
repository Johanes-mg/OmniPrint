use mdns_sd::{ServiceDaemon, ServiceInfo};

pub fn publier(port: u16) -> Option<ServiceDaemon> {
    let daemon = ServiceDaemon::new().ok()?;
    let hostname = "omniprint.local.";
    let instance = "OmniPrint";
    let service_type = "_http._tcp.local.";
    let properties = [("path", "/"), ("version", env!("CARGO_PKG_VERSION"))];
    let ip = crate::reseau_utils::adresse_ip_locale();
    let info = ServiceInfo::new(service_type, instance, hostname, &ip, port, &properties[..]).ok()?;
    daemon.register(info).ok()?;
    Some(daemon)
}
