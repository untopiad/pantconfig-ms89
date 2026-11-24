#[cfg(feature = "gui")]
fn main() -> eframe::Result<()> {
    panteon_ms89::gui::run_gui()
}

#[cfg(not(feature = "gui"))]
fn main() {
    eprintln!("panteon-gui binary requires the 'gui' feature: cargo build --bin panteon-gui --features gui");
}
