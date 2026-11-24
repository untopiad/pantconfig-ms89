use std::fmt;

use crate::codec::{Command, CONFIG_UNLOCK};
use crate::model::{DeviceIdentity, PackedMemMode};
use crate::{PanteonError, FEATURE_REPORT_LEN, REPORT_ID};

/// Multi-tier hardware write safety gate (ADR-001)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum WriteGateLevel {
    /// Read-only mode (Default): all hardware writes are rejected
    #[default]
    Level0ReadOnly = 0,
    /// Addressing only: permits prep/readback sequence frames (modes 0x09/0x05/0x00), blocks data writes (mode 0x03)
    Level1AddressingOnly = 1,
    /// Hardware write mode: allows full multi-phase write transactions with Interrupt IN ACK verification
    Level2ConfigurationWrite = 2,
}

impl WriteGateLevel {
    pub fn authorized(&self, required: WriteGateLevel) -> bool {
        *self >= required
    }
}

impl fmt::Display for WriteGateLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Level0ReadOnly => write!(f, "Level 0 (Read-Only)"),
            Self::Level1AddressingOnly => write!(f, "Level 1 (Addressing Only)"),
            Self::Level2ConfigurationWrite => write!(f, "Level 2 (Hardware Write)"),
        }
    }
}

/// Validate report against current Write-Gate level and identity verification rules.
pub fn check_write_gate(
    gate_level: WriteGateLevel,
    identity: &DeviceIdentity,
    report: &[u8; FEATURE_REPORT_LEN],
) -> Result<(), PanteonError> {
    match gate_level {
        WriteGateLevel::Level0ReadOnly => Err(PanteonError::WriteDisabled),
        WriteGateLevel::Level1AddressingOnly => {
            if report[0] != REPORT_ID {
                return Err(PanteonError::Protocol("Invalid report ID for write"));
            }
            if report[1] == Command::PackedMem as u8 && report[2] == PackedMemMode::Write as u8 {
                return Err(PanteonError::WriteDisabled);
            }
            if report[1] == Command::PackedMem as u8 {
                let addr = u16::from_le_bytes([report[4], report[5]]);
                if addr == CONFIG_UNLOCK.1 && report[2] != crate::codec::READ_REQUEST_MODE {
                    return Err(PanteonError::WriteDisabled);
                }
            }
            Ok(())
        }
        WriteGateLevel::Level2ConfigurationWrite => {
            if !identity.protocol_verified {
                return Err(PanteonError::ProtocolDetailed {
                    context: "write_feature",
                    detail: format!(
                        "protocol unverified for PID 0x{:04x} — run verify-protocol first",
                        identity.pid
                    ),
                });
            }
            Ok(())
        }
    }
}

