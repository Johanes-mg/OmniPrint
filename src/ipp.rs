use ipp_printer_app::device::{DeviceBackend, PollStatus};
use ipp_printer_app::printer::PrinterConfig;

pub struct OmniPrintBackend;

impl OmniPrintBackend {
    pub fn new() -> Self {
        Self
    }
}

impl DeviceBackend for OmniPrintBackend {
    fn list(&self, emit: &mut dyn FnMut(&str, &str, &str) -> bool) {
        emit(
            "OmniPrint Virtual Printer",
            "omniprint://local",
            "MFG:OmniPrint;MDL:Virtual;",
        );
    }

    fn driver_for_device(&self, _device_id: &str, _device_uri: &str) -> Option<String> {
        Some("omniprint-raster".into())
    }

    fn poll_status(&self, _config: &PrinterConfig) -> Option<PollStatus> {
        Some(PollStatus::default())
    }

    fn identify(&self, _config: &PrinterConfig, _actions: &[String]) {}
}
