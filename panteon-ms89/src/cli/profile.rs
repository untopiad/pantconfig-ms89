use std::fs;
use std::path::PathBuf;

use panteon_ms89::model::Profile;
use panteon_ms89::store::{parse_toml, to_toml_string};
use panteon_ms89::PanteonError;

pub fn run_export(output: Option<PathBuf>, profile: u8) -> Result<(), PanteonError> {
    let prof = Profile {
        id: profile,
        name: format!("Config{}", profile),
        ..Default::default()
    };

    let toml_data = to_toml_string(&prof);
    match output {
        Some(path) => {
            fs::write(&path, &toml_data).map_err(|e| {
                PanteonError::Io(format!("Failed to write {}: {}", path.display(), e))
            })?;
            println!("Exported profile '{}' to {}", prof.name, path.display());
        }
        None => {
            print!("{}", toml_data);
        }
    }
    Ok(())
}

pub fn run_import(path: PathBuf) -> Result<(), PanteonError> {
    let content = fs::read_to_string(&path).map_err(|e| {
        PanteonError::Io(format!("Failed to read {}: {}", path.display(), e))
    })?;
    let prof = parse_toml(&content)?;

    println!("Successfully parsed and validated profile:");
    println!("  Name: {} (ID: {})", prof.name, prof.id);
    println!("  Active DPI Index: {}", prof.active_dpi_index);
    println!("  Polling Rate: {} Hz", prof.polling_rate.as_hz());
    println!("  DPI Levels: {} configured", prof.dpi_levels.len());
    for dpi in &prof.dpi_levels {
        println!(
            "    [{}] {} DPI (enabled: {}, RGB: [{}, {}, {}], second: [{}, {}, {}])",
            dpi.level_index,
            dpi.dpi,
            dpi.enabled,
            dpi.main[0], dpi.main[1], dpi.main[2],
            dpi.second[0], dpi.second[1], dpi.second[2],
        );
    }
    println!("  Button Bindings: {} buttons", prof.button_bindings.len());
    for btn in &prof.button_bindings {
        println!("    Button {}: {}", btn.button_index, btn.action.as_str());
    }
    println!(
        "\n[SAFETY NOTICE] Device writes are disabled until protocol verification (ADR-001)."
    );
    println!("Profile validated successfully, but was NOT written to device.");
    Ok(())
}
