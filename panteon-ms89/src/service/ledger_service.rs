use std::path::{Path, PathBuf};

use crate::model::{current_unix_timestamp, ShadowEntry, ShadowLedger};
use crate::store::{parse_ledger, to_toml_string};
use crate::PanteonError;

/// Pure function to append a shadow entry into a ledger
pub fn append_entry(ledger: &mut ShadowLedger, entry: ShadowEntry) {
    ledger.append(entry);
}

/// Resolve state path: custom path > $XDG_STATE_HOME/panteon-ms89/ledger.toml > ~/.local/state/panteon-ms89/ledger.toml
pub fn resolve_state_path(custom: Option<PathBuf>) -> PathBuf {
    if let Some(path) = custom {
        return path;
    }
    if let Ok(xdg_state) = std::env::var("XDG_STATE_HOME") {
        if !xdg_state.trim().is_empty() {
            return PathBuf::from(xdg_state).join("panteon-ms89").join("ledger.toml");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("panteon-ms89")
                .join("ledger.toml");
        }
    }
    PathBuf::from("ledger.toml")
}

/// Load ShadowLedger from file, returning empty ledger if file does not exist
pub fn load_ledger(path: &Path) -> Result<ShadowLedger, PanteonError> {
    if !path.exists() {
        return Ok(ShadowLedger::default());
    }
    let content = std::fs::read_to_string(path).map_err(|e| PanteonError::Io(e.to_string()))?;
    parse_ledger(&content)
}

/// Atomically write ShadowLedger to path via temp file and rename
pub fn save_ledger_atomic(path: &Path, ledger: &ShadowLedger) -> Result<(), PanteonError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PanteonError::Io(e.to_string()))?;
    }
    let toml_data = to_toml_string(ledger);
    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp_path, toml_data).map_err(|e| PanteonError::Io(e.to_string()))?;
    std::fs::rename(&tmp_path, path).map_err(|e| PanteonError::Io(e.to_string()))?;
    Ok(())
}

/// Record a successful write into the shadow ledger at state_path
pub fn record_write(
    state_path: &Path,
    device_hint: &Path,
    bank: u8,
    addr: u16,
    data: u16,
) -> Result<(), PanteonError> {
    let mut ledger = load_ledger(state_path).unwrap_or_default();
    ledger.device_hint = device_hint.display().to_string();
    let entry = ShadowEntry::new(bank, addr, data, current_unix_timestamp());
    append_entry(&mut ledger, entry);
    save_ledger_atomic(state_path, &ledger)
}
