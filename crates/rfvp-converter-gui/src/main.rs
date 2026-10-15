#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => {}
        [arg] if arg == "--version" => {
            println!("rfvp-converter-gui {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        [arg] if arg == "--help" || arg == "-h" => {
            println!(
                "RFVP Converter: native desktop game conversion\n\nUsage: rfvp-converter-gui [--version | --help]"
            );
            return Ok(());
        }
        _ => {
            eprintln!("Unknown arguments. Use --help or launch without arguments.");
            std::process::exit(2);
        }
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([920.0, 790.0])
            .with_min_inner_size([680.0, 540.0])
            .with_app_id("org.rfvp.converter"),
        renderer: eframe::Renderer::Glow,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "RFVP Converter",
        options,
        Box::new(|cc| Ok(Box::new(app::ConverterApp::new(cc)))),
    )
}
