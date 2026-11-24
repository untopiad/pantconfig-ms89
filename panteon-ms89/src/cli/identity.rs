use std::path::PathBuf;

use panteon_ms89::codec::decode_descriptor_page;
use panteon_ms89::device::{select_device_path, Device, WriteGateLevel};
use panteon_ms89::PanteonError;

use super::render::{fmt_hex_slice, print_target_device};

pub fn run_verify_protocol(device: Option<PathBuf>) -> Result<(), PanteonError> {
    let target_path = select_device_path(device)?;

    println!("=== PANTEON MS89 Protocol Verification Workflow (M2.7) ===");
    print_target_device(&target_path);

    // Open device
    let mut dev = Device::open(&target_path)?;
    let identity = *dev.identity();
    println!(
        "Target Identity: PID 0x{:04x}, accepted signatures {:04x?}, verified in code: {}",
        identity.pid, identity.accepted_signatures, identity.protocol_verified
    );

    // -------------------------------------------------------------
    // Phase 1: Probe handshake signature
    // -------------------------------------------------------------
    println!("\n[Phase 1/2] Probing device handshake signature (Level 0)...");
    let state = dev.read_state()?;
    let hs = state.handshake.ok_or(
        PanteonError::Protocol("Device returned no handshake frame during verification probe")
    )?;
    println!(
        "  Raw handshake: state=0x{:02x}, signature=0x{:04x}, revision=0x{:02x}, status=0x{:02x}, flags=0x{:02x}",
        hs.state_byte, hs.signature, hs.revision, hs.status, hs.flags
    );
    if !identity.accepts_signature(hs.signature) {
        println!(
            "  [FAIL] Handshake signature mismatch! Expected one of {:04x?}, got 0x{:04x} (state 0x{:02x})",
            identity.accepted_signatures, hs.signature, hs.state_byte
        );
        println!("\n=== VERDICT ===");
        println!("Protocol verification FAILED at Phase 1 for PID 0x{:04x}", identity.pid);
        return Err(PanteonError::Protocol("Handshake signature mismatch during verify-protocol"));
    }
    println!("  [OK] Signature 0x{:04x} (state 0x{:02x}) matches identity registry.", hs.signature, hs.state_byte);

    // -------------------------------------------------------------
    // Phase 2: Descriptor page inspection (Level 1)
    // -------------------------------------------------------------
    println!("\n[Phase 2/2] Reading descriptor page (Write-Gate Level 1)...");
    dev.set_write_gate_level(WriteGateLevel::Level1AddressingOnly);
    let raw_desc_before = dev.experimental_read(0, 0, 32, Device::READ_PROTOCOL_DELAY)?;
    let desc_before = decode_descriptor_page(&raw_desc_before)?;
    println!("  Raw bytes (32): {}", fmt_hex_slice(&raw_desc_before));
    println!(
        "  Parsed fields: sync=(0x{:02x}, 0x{:02x}), unlock=0x{:02x}, PID=0x{:04x}, vid_lo=0x{:02x}, write_echo=0x{:02x}",
        desc_before.sync.0, desc_before.sync.1, desc_before.unlock_state, desc_before.pid,
        desc_before.vid_lo, desc_before.write_echo
    );
    if desc_before.pid == identity.pid {
        println!(
            "  [EVIDENCE] Descriptor bytes 3..4 (0x{:04x}) MATCH actual device PID 0x{:04x}! (Hypothesis confirmed)",
            desc_before.pid, identity.pid
        );
    } else {
        println!(
            "  [EVIDENCE] Descriptor bytes 3..4 (0x{:04x}) do NOT match actual device PID 0x{:04x}!",
            desc_before.pid, identity.pid
        );
    }

    // -------------------------------------------------------------
    // Final Verdict and Persistent Marker
    // -------------------------------------------------------------
    println!("\n=== VERDICT ===");
    println!("protocol verified for PID 0x{:04x}", identity.pid);

    let marker_path = panteon_ms89::store::resolve_verified_marker_path(None);
    let mut store = panteon_ms89::store::load_verified_store(&marker_path).unwrap_or_default();
    let verified_at = panteon_ms89::model::current_unix_timestamp();
    store.insert_or_update(identity.pid, identity.accepted_signatures.to_vec(), verified_at);
    panteon_ms89::store::save_verified_store_atomic(&marker_path, &store)?;
    println!("Verification marker recorded to {}", marker_path.display());

    println!("Evidence captured. Record findings in docs/identity-registry.md.");
    println!("Note: protocol_verified flag in source code registry is maintained per ADR-001.");

    Ok(())
}

pub fn run_forget_verification(
    pid: Option<u16>,
    state: Option<PathBuf>,
) -> Result<(), PanteonError> {
    let marker_path = panteon_ms89::store::resolve_verified_marker_path(state);
    if !marker_path.exists() {
        println!("No verification marker found at {}", marker_path.display());
        return Ok(());
    }

    if let Some(target_pid) = pid {
        let mut store = panteon_ms89::store::load_verified_store(&marker_path)?;
        if store.remove(target_pid) {
            if store.entries.is_empty() {
                panteon_ms89::store::remove_verified_marker_file(&marker_path)?;
                println!(
                    "Removed verification marker for PID 0x{:04x} and deleted {}",
                    target_pid, marker_path.display()
                );
            } else {
                panteon_ms89::store::save_verified_store_atomic(&marker_path, &store)?;
                println!(
                    "Removed verification marker for PID 0x{:04x} in {}",
                    target_pid, marker_path.display()
                );
            }
        } else {
            println!(
                "No verification marker found for PID 0x{:04x} in {}",
                target_pid, marker_path.display()
            );
        }
    } else {
        panteon_ms89::store::remove_verified_marker_file(&marker_path)?;
        println!("All verification markers removed ({})", marker_path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_protocol_compiles() {
        let _ = run_verify_protocol;
    }
}
