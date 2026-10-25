#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod audio;
mod ble;
mod firmware;
mod localization;
mod model;
mod platform;
mod protocol;
mod rfcomm;
mod ui;
mod worker;
const APP_NAME: &str = "Bose Headphone Control";
const APP_VERSION: &str = env!("APP_DISPLAY_VERSION");

struct DiagnosticLogger;
impl log::Log for DiagnosticLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            worker::log(&format!("{} {}", r.target(), r.args()));
        }
    }
    fn flush(&self) {}
}
static DIAGNOSTIC_LOGGER: DiagnosticLogger = DiagnosticLogger;
fn main() {
    let _ = log::set_logger(&DIAGNOSTIC_LOGGER);
    log::set_max_level(log::LevelFilter::Warn);
    if let Err(error) = run() {
        worker::log(&format!("Startup: {error:#}"));
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
fn run() -> anyhow::Result<()> {
    platform::set_app_identity()?;
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--quit") {
        platform::signal(false)?;
        return Ok(());
    }
    let _instance = match platform::Instance::acquire()? {
        Some(instance) => instance,
        None => return Ok(()),
    };
    worker::log(&format!("{APP_NAME} {APP_VERSION}"));
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1080., 800.])
            .with_min_inner_size([820., 680.])
            .with_maximized(args.iter().any(|a| a == "--maximized"))
            .with_icon(ui::app_icon()),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            supported_backends: wgpu::Backends::DX12,
            desired_maximum_frame_latency: Some(1),
            ..Default::default()
        },
        vsync: true,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(ui::Dashboard::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}
