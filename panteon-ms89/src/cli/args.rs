use clap::Args;
use panteon_ms89::device::WriteGateLevel;

pub fn parse_hex_or_dec_u8(s: &str) -> Result<u8, String> {
    let s = s.trim();
    s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))
        .map(|h| u8::from_str_radix(h, 16).map_err(|e| format!("Invalid hex bank: {}", e)))
        .unwrap_or_else(|| s.parse::<u8>().or_else(|_| u8::from_str_radix(s, 16)).map_err(|e| format!("Invalid bank '{}': {}", s, e)))
}

pub fn parse_hex_or_dec_u16(s: &str) -> Result<u16, String> {
    let s = s.trim();
    s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))
        .map(|h| u16::from_str_radix(h, 16).map_err(|e| format!("Invalid hex address: {}", e)))
        .unwrap_or_else(|| s.parse::<u16>().or_else(|_| u16::from_str_radix(s, 16)).map_err(|e| format!("Invalid address '{}': {}", s, e)))
}

/// Parses an RGB hex color argument (e.g. '#ff8800' or 'ff8800') for CLI flags.
pub fn parse_hex_color_arg(s: &str) -> Result<(u8, u8, u8), String> {
    crate::cli::dpi::parse_hex_color(s).map_err(|e| e.to_string())
}

/// Write-gate security argument group for CLI subcommands
#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
pub struct GateArgs {
    /// Confirm execution of hardware operation
    #[arg(long)]
    pub confirm: bool,
    /// Authorize Level 1 unsafe addressing write
    #[arg(long)]
    pub unsafe_addressing: bool,
    /// Authorize Level 2 hardware write
    #[arg(long)]
    pub write: bool,
    /// Allow hardware write to unverified device identity (requires --confirm --write)
    #[arg(long)]
    pub allow_unverified_device: bool,
}

impl GateArgs {
    pub fn level(&self) -> WriteGateLevel {
        if self.write && self.confirm {
            WriteGateLevel::Level2ConfigurationWrite
        } else if self.unsafe_addressing {
            WriteGateLevel::Level1AddressingOnly
        } else {
            WriteGateLevel::Level0ReadOnly
        }
    }

    pub fn authorized(&self, required: WriteGateLevel) -> bool {
        match required {
            WriteGateLevel::Level0ReadOnly => true,
            WriteGateLevel::Level1AddressingOnly => self.confirm && self.level() >= WriteGateLevel::Level1AddressingOnly,
            WriteGateLevel::Level2ConfigurationWrite => self.level() >= WriteGateLevel::Level2ConfigurationWrite,
        }
    }

    /// Check if the allow-unverified-device escape is authorized by --confirm --write
    pub fn is_unverified_escape_valid(&self) -> bool {
        self.allow_unverified_device && self.confirm && self.write
    }

    /// Validate that --allow-unverified-device is not specified without --confirm --write
    pub fn validate_unverified_escape(&self) -> Result<(), panteon_ms89::PanteonError> {
        if self.allow_unverified_device && !self.is_unverified_escape_valid() {
            return Err(panteon_ms89::PanteonError::ProtocolDetailed {
                context: "gate_authorization",
                detail: "--allow-unverified-device requires --confirm and --write".to_string(),
            });
        }
        Ok(())
    }
}

/// Authorize write on target device, handling identity verification and --allow-unverified-device escape hatch
pub fn authorize_device_write(
    dev: &mut panteon_ms89::device::Device,
    gate: &GateArgs,
) -> Result<(), panteon_ms89::PanteonError> {
    gate.validate_unverified_escape()?;
    if !dev.identity().protocol_verified {
        if gate.is_unverified_escape_valid() {
            println!("*** UNVERIFIED DEVICE WRITE *** (authorized by --allow-unverified-device)");
            eprintln!(
                "WARNING: Target device PID 0x{:04x} is unverified. Proceeding under operator responsibility.",
                dev.identity().pid
            );
            let mut id = *dev.identity();
            id.protocol_verified = true;
            dev.set_identity(id);
        } else {
            return Err(panteon_ms89::PanteonError::ProtocolDetailed {
                context: "write_feature",
                detail: format!(
                    "protocol unverified for PID 0x{:04x} — run verify-protocol first",
                    dev.identity().pid
                ),
            });
        }
    }
    Ok(())
}

/// Print warning in dry-run mode if the device is unverified
pub fn print_unverified_dry_run_warning_if_needed(dev: &panteon_ms89::device::Device) {
    if !dev.identity().protocol_verified {
        println!(
            "WARNING: Target device PID 0x{:04x} is UNVERIFIED! To execute write on this device, pass --allow-unverified-device together with --confirm --write.",
            dev.identity().pid
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gate_args_level_and_authorization() {
        let default_args = GateArgs::default();
        assert_eq!(default_args.level(), WriteGateLevel::Level0ReadOnly);
        assert!(default_args.authorized(WriteGateLevel::Level0ReadOnly));
        assert!(!default_args.authorized(WriteGateLevel::Level1AddressingOnly));
        assert!(!default_args.authorized(WriteGateLevel::Level2ConfigurationWrite));
        let l1_no_confirm = GateArgs { unsafe_addressing: true, ..Default::default() };
        assert_eq!(l1_no_confirm.level(), WriteGateLevel::Level1AddressingOnly);
        assert!(!l1_no_confirm.authorized(WriteGateLevel::Level1AddressingOnly));
        let l1_full = GateArgs { confirm: true, unsafe_addressing: true, ..Default::default() };
        assert_eq!(l1_full.level(), WriteGateLevel::Level1AddressingOnly);
        assert!(l1_full.authorized(WriteGateLevel::Level1AddressingOnly));
        let l2_write_without_confirm = GateArgs { write: true, ..Default::default() };
        assert_eq!(l2_write_without_confirm.level(), WriteGateLevel::Level0ReadOnly);
        assert!(!l2_write_without_confirm.authorized(WriteGateLevel::Level2ConfigurationWrite));
        let l2_full = GateArgs { confirm: true, write: true, ..Default::default() };
        assert_eq!(l2_full.level(), WriteGateLevel::Level2ConfigurationWrite);
        assert!(l2_full.authorized(WriteGateLevel::Level2ConfigurationWrite));

        // Test allow_unverified_device escape hatch validation
        let escape_without_triplet = GateArgs {
            allow_unverified_device: true,
            confirm: true,
            ..Default::default()
        };
        assert!(!escape_without_triplet.is_unverified_escape_valid());
        assert!(escape_without_triplet.validate_unverified_escape().is_err());

        let escape_full = GateArgs {
            allow_unverified_device: true,
            confirm: true,
            write: true,
            ..Default::default()
        };
        assert!(escape_full.is_unverified_escape_valid());
        assert!(escape_full.validate_unverified_escape().is_ok());
    }
}
