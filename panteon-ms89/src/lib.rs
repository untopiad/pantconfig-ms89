extern crate self as panteon_ms89;
pub mod cli;
pub mod codec;
pub mod device;
pub mod model;
pub mod capture;
pub use capture as snapshot;
pub mod store;
pub mod service;
#[cfg(feature = "gui")]
pub mod gui;

pub const VID: u16 = 0x30fa;
pub const PID: u16 = 0x1440;
pub const REPORT_ID: u8 = 0x07;
/// Hardware HID Feature Report size for PANTEON MS89 Interface 1 (Report ID + 7 data bytes = 8 bytes)
pub const FEATURE_REPORT_LEN: usize = 8;
pub const FEATURE_PAYLOAD_LEN: usize = 7;

/// Errors for PANTEON MS89 operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanteonError {
    WriteDisabled,
    DeviceNotFound,
    Io(String),
    Protocol(&'static str),
    ProtocolDetailed {
        context: &'static str,
        detail: String,
    },
}

impl std::fmt::Display for PanteonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WriteDisabled => write!(f, "Device write operations are strictly disabled until protocol verification"),
            Self::DeviceNotFound => write!(f, "No matching PANTEON MS89 device found"),
            Self::Io(msg) => write!(f, "I/O error: {}", msg),
            Self::Protocol(msg) => write!(f, "Protocol error: {}", msg),
            Self::ProtocolDetailed { context, detail } => {
                write!(f, "Protocol error in {}: {}", context, detail)
            }
        }
    }
}

impl std::error::Error for PanteonError {}

pub use codec::{
    decode_command, decode_descriptor_page, decode_get_report_response, decode_packed_mem,
    decode_profile_block, decode_read_response, decode_value16, encode_0x15, encode_0x16,
    encode_0x17, encode_0x20, encode_command, encode_config_unlock,
    encode_config_unlock_commit_bank, encode_config_unlock_final_commit, encode_get,
    encode_macro_delay, encode_packed_mem, encode_packed_mem_mode_0x10, encode_page_base,
    encode_probe, encode_probe_command, encode_profile_block, encode_read_bank_arm_finalize,
    encode_read_bank_frame, encode_read_bank_prep_frame, encode_read_request,
    encode_read_request_0x13, encode_read_request_mode, encode_read_reset,
    encode_read_reset_frame, encode_read_sweep_frame, encode_value16, parse_hid_feature_report_size,
    Command, DescriptorPage, DeviceGetReportResponse, ReadPhase, ReadSequenceFrame, CONFIG_UNLOCK,
    HYPOTHETICAL_PAGE_BASE_PROBE, LIVE_DESCRIPTOR_PAGE_BYTES, READ_REQUEST_MODE,
    READ_REQUEST_MODE_08_REFUTED,
};
pub use device::{
    discover_devices, parse_uevent_vid_pid, select_device_path, BlockWrite, Device, DiscoveredDevice,
    Transport, WriteGateLevel, WriteTransactionPlan,
};
#[allow(deprecated)]
pub use model::{
    current_timestamp_secs, current_unix_timestamp, registers, ButtonAction, ButtonBinding, DeviceIdentity, DeviceState,
    DpiLevel, DpiSlot, PackedMemLayout, PackedMemMode, PollingRate, Profile, ShadowEntry, ShadowLedger,
    StateConfidence, StateSnapshot, ACCEPTED_SIGNATURES, KNOWN_IDENTITIES,
};
pub use capture::{parse_capture_feature_sets, parse_pcapng_feature_sets};
pub use store::{
    ledger_to_toml_string, parse as parse_toml_profile, parse_ledger, parse_snapshot, parse_toml,
    snapshot_to_toml_string, to_toml_string, ToToml,
};
