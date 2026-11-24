use std::path::{Path, PathBuf};

use crate::PanteonError;

/// Entry representing a verified device protocol on target hardware
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedEntry {
    pub pid: u16,
    pub signatures: Vec<u16>,
    pub verified_at: u64,
}

/// Store holding all persisted device protocol verifications
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerifiedStore {
    pub entries: Vec<VerifiedEntry>,
}

impl VerifiedStore {
    pub fn find(&self, pid: u16) -> Option<&VerifiedEntry> {
        self.entries.iter().find(|e| e.pid == pid)
    }

    pub fn insert_or_update(&mut self, pid: u16, signatures: Vec<u16>, verified_at: u64) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.pid == pid) {
            existing.signatures = signatures;
            existing.verified_at = verified_at;
        } else {
            self.entries.push(VerifiedEntry {
                pid,
                signatures,
                verified_at,
            });
        }
    }

    pub fn remove(&mut self, pid: u16) -> bool {
        let initial_len = self.entries.len();
        self.entries.retain(|e| e.pid != pid);
        self.entries.len() != initial_len
    }
}

/// Parse string into u16 accepting either hex (0x...) or decimal notation
pub fn parse_hex_or_dec_u16(s: &str) -> Result<u16, String> {
    let s = s.trim();
    s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .map(|h| u16::from_str_radix(h, 16).map_err(|e| format!("Invalid hex address: {}", e)))
        .unwrap_or_else(|| {
            s.parse::<u16>()
                .or_else(|_| u16::from_str_radix(s, 16))
                .map_err(|e| format!("Invalid integer '{}': {}", s, e))
        })
}

/// Serialize VerifiedStore into minimal TOML string format
pub fn verified_store_to_toml(store: &VerifiedStore) -> String {
    let mut out = String::new();
    for entry in &store.entries {
        out.push_str("[[verified]]\n");
        out.push_str(&format!("pid = 0x{:04x}\n", entry.pid));
        let sigs_str = entry
            .signatures
            .iter()
            .map(|s| format!("\"0x{:04x}\"", s))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("signatures = [{}]\n", sigs_str));
        out.push_str(&format!("verified_at = {}\n\n", entry.verified_at));
    }
    out
}

#[derive(Default)]
struct EntryBuilder {
    pid: Option<u16>,
    signatures: Option<Vec<u16>>,
    verified_at: Option<u64>,
}

impl EntryBuilder {
    fn build(self) -> Result<VerifiedEntry, PanteonError> {
        let pid = self.pid.ok_or(PanteonError::ProtocolDetailed {
            context: "VerifiedEntry",
            detail: "missing required field 'pid'".to_string(),
        })?;
        let signatures = self.signatures.ok_or(PanteonError::ProtocolDetailed {
            context: "VerifiedEntry",
            detail: "missing required field 'signatures'".to_string(),
        })?;
        let verified_at = self.verified_at.ok_or(PanteonError::ProtocolDetailed {
            context: "VerifiedEntry",
            detail: "missing required field 'verified_at'".to_string(),
        })?;
        Ok(VerifiedEntry {
            pid,
            signatures,
            verified_at,
        })
    }
}

/// Parse TOML string representation of verified marker
pub fn parse_verified_store(content: &str) -> Result<VerifiedStore, PanteonError> {
    let mut entries = Vec::new();
    let mut current_builder: Option<EntryBuilder> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed == "[[verified]]" {
            if let Some(builder) = current_builder.take() {
                entries.push(builder.build()?);
            }
            current_builder = Some(EntryBuilder::default());
            continue;
        }

        if let Some(builder) = current_builder.as_mut() {
            let mut parts = trimmed.splitn(2, '=');
            let key = parts.next().ok_or(PanteonError::Protocol("Missing key"))?.trim();
            let val = parts.next().ok_or(PanteonError::Protocol("Missing value"))?.trim();

            match key {
                "pid" => {
                    let pid_val = parse_hex_or_dec_u16(val)
                        .map_err(|e| PanteonError::ProtocolDetailed {
                            context: "parse_verified_store",
                            detail: format!("Failed to parse pid: {}", e),
                        })?;
                    builder.pid = Some(pid_val);
                }
                "signatures" => {
                    let inner = val
                        .trim_start_matches('[')
                        .trim_end_matches(']')
                        .trim();
                    let mut sigs = Vec::new();
                    if !inner.is_empty() {
                        for item in inner.split(',') {
                            let clean = item.trim().trim_matches('"').trim_matches('\'').trim();
                            if !clean.is_empty() {
                                let sig = parse_hex_or_dec_u16(clean)
                                    .map_err(|e| PanteonError::ProtocolDetailed {
                                        context: "parse_verified_store",
                                        detail: format!("Failed to parse signature '{}': {}", clean, e),
                                    })?;
                                sigs.push(sig);
                            }
                        }
                    }
                    builder.signatures = Some(sigs);
                }
                "verified_at" => {
                    let ts = val.parse::<u64>().map_err(|e| PanteonError::ProtocolDetailed {
                        context: "parse_verified_store",
                        detail: format!("Failed to parse verified_at timestamp: {}", e),
                    })?;
                    builder.verified_at = Some(ts);
                }
                _ => {}
            }
        }
    }

    if let Some(builder) = current_builder.take() {
        entries.push(builder.build()?);
    }

    Ok(VerifiedStore { entries })
}

/// Resolve path to verified.toml: custom > $XDG_STATE_HOME > ~/.local/state > verified.toml
pub fn resolve_verified_marker_path(custom: Option<PathBuf>) -> PathBuf {
    if let Some(path) = custom {
        return path;
    }
    if let Ok(xdg_state) = std::env::var("XDG_STATE_HOME") {
        if !xdg_state.trim().is_empty() {
            return PathBuf::from(xdg_state).join("panteon-ms89").join("verified.toml");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("panteon-ms89")
                .join("verified.toml");
        }
    }
    PathBuf::from("verified.toml")
}

/// Load VerifiedStore from file, returning empty store if file does not exist
pub fn load_verified_store(path: &Path) -> Result<VerifiedStore, PanteonError> {
    if !path.exists() {
        return Ok(VerifiedStore::default());
    }
    let content = std::fs::read_to_string(path).map_err(|e| PanteonError::Io(e.to_string()))?;
    parse_verified_store(&content)
}

/// Atomically write VerifiedStore to path via temp file and rename
pub fn save_verified_store_atomic(path: &Path, store: &VerifiedStore) -> Result<(), PanteonError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PanteonError::Io(e.to_string()))?;
    }
    let toml_data = verified_store_to_toml(store);
    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp_path, toml_data).map_err(|e| PanteonError::Io(e.to_string()))?;
    std::fs::rename(&tmp_path, path).map_err(|e| PanteonError::Io(e.to_string()))?;
    Ok(())
}

/// Remove verified marker file from disk
pub fn remove_verified_marker_file(path: &Path) -> Result<bool, PanteonError> {
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| PanteonError::Io(e.to_string()))?;
        Ok(true)
    } else {
        Ok(false)
    }
}
