pub mod ledger;
pub mod presets;
pub mod profile;
pub mod snapshot_store;
pub mod verified;

pub use ledger::{ledger_to_toml_string, parse_ledger};
pub use profile::{parse, parse_toml, profile_to_toml_string};
pub use snapshot_store::{parse_snapshot, snapshot_to_toml_string};
pub use verified::{
    load_verified_store, parse_verified_store, remove_verified_marker_file,
    resolve_verified_marker_path, save_verified_store_atomic, verified_store_to_toml, VerifiedEntry,
    VerifiedStore,
};

use crate::model::{Profile, ShadowLedger, StateSnapshot};
use crate::PanteonError;

/// Trait for minimal TOML serialization
pub trait ToToml {
    fn to_toml_string(&self) -> String;
}

impl ToToml for Profile {
    fn to_toml_string(&self) -> String {
        profile_to_toml_string(self)
    }
}

impl ToToml for ShadowLedger {
    fn to_toml_string(&self) -> String {
        ledger_to_toml_string(self)
    }
}

impl ToToml for StateSnapshot {
    fn to_toml_string(&self) -> String {
        snapshot_to_toml_string(self)
    }
}

/// Serialize any ToToml type into minimal TOML string format
pub fn to_toml_string<T: ToToml + ?Sized>(item: &T) -> String {
    item.to_toml_string()
}

pub(crate) fn parse_key_value(line: &str) -> Result<(&str, &str), PanteonError> {
    let mut parts = line.splitn(2, '=');
    let key = parts.next().ok_or(PanteonError::Protocol("Missing key"))?.trim();
    let val = parts.next().ok_or(PanteonError::Protocol("Missing value"))?.trim();
    Ok((key, val))
}

pub(crate) fn strip_quotes(s: &str) -> &str {
    s.trim().trim_matches('"')
}
