use std::path::PathBuf;

use panteon_ms89::codec::{encode_read_request, HYPOTHETICAL_PAGE_BASE_PROBE};
use panteon_ms89::device::{select_device_path, Device, Transport, WriteGateLevel};
use panteon_ms89::model::registers;
use panteon_ms89::{PanteonError, FEATURE_REPORT_LEN};

use super::args::{authorize_device_write, print_unverified_dry_run_warning_if_needed, GateArgs};

pub fn run_experimental_read_probe(
    device: Option<PathBuf>,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let probe_13 = HYPOTHETICAL_PAGE_BASE_PROBE;
    let probe_18 = encode_read_request(registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1);

    println!("[DRY-RUN] Candidate addressing frames for experimental read probe:");
    println!("  Candidate 1 (PageBase 0x13): {:02x?}", &probe_13);
    println!("  Candidate 2 (PackedMem Mode 0x01): {:02x?}", &probe_18);

    if gate.authorized(WriteGateLevel::Level1AddressingOnly) {
        let target_path = select_device_path(device)?;

        println!(
            "[Level 1 Authorized] Transmitting candidate addressing probe to {}...",
            target_path.display()
        );
        let mut dev =
            Device::open_with_gate_level(&target_path, WriteGateLevel::Level1AddressingOnly)?;
        dev.write_feature(&probe_13)?;
        println!("Candidate 1 transmitted under Level 1. Probe sequence complete.");
    } else {
        println!(
            "\nDry-run completed. To transmit candidate addressing frames to hardware, specify --confirm AND --unsafe-addressing."
        );
    }
    Ok(())
}

pub fn run_raw_frame(
    hex_data: String,
    device: Option<PathBuf>,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let cleaned: String = hex_data
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();
    if cleaned.len() != FEATURE_REPORT_LEN * 2 {
        eprintln!(
            "Error: hex_data must be exactly {} bytes ({} hex chars), got {} chars",
            FEATURE_REPORT_LEN,
            FEATURE_REPORT_LEN * 2,
            cleaned.len()
        );
        return Err(PanteonError::Protocol("Invalid raw frame length"));
    }

    let mut frame = [0u8; FEATURE_REPORT_LEN];
    for i in 0..FEATURE_REPORT_LEN {
        frame[i] = u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16)
            .map_err(|_| PanteonError::Protocol("Failed to parse hex byte"))?;
    }

    println!("[RAW-FRAME] Parsed candidate frame: {:02x?}", &frame);

    if gate.confirm && gate.authorized(WriteGateLevel::Level2ConfigurationWrite) {
        let target_path = select_device_path(device)?;

        println!(
            "[Level 2 Authorized] Transmitting raw feature report to {}...",
            target_path.display()
        );
        let mut dev = Device::open_with_gate_level(
            &target_path,
            WriteGateLevel::Level2ConfigurationWrite,
        )?;
        authorize_device_write(&mut dev, &gate)?;
        match dev.write_feature(&frame) {
            Ok(()) => {
                println!("Result: SUCCESS (frame accepted by hardware, no STALL).");
            }
            Err(e) => {
                eprintln!("Result: FAILED: {}", e);
                return Err(e);
            }
        }
    } else {
        if let Ok(target) = select_device_path(device) {
            if let Ok(dev) = Device::open_with_gate_level(&target, WriteGateLevel::Level0ReadOnly) {
                print_unverified_dry_run_warning_if_needed(&dev);
            }
        }
        println!(
            "\nDry-run completed. To transmit raw frame to hardware, specify --confirm AND --write (Level 2)."
        );
    }
    Ok(())
}

pub fn run_probe_command(
    cmd: u8,
    args: String,
    device: Option<PathBuf>,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let cleaned: String = args.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if cleaned.len() % 2 != 0 {
        return Err(PanteonError::Protocol("Odd number of hex digits in args"));
    }
    let mut arg_bytes = Vec::new();
    for i in (0..cleaned.len()).step_by(2) {
        let b = u8::from_str_radix(&cleaned[i..i + 2], 16)
            .map_err(|_| PanteonError::Protocol("Failed to parse hex arg byte"))?;
        arg_bytes.push(b);
    }

    let frame = panteon_ms89::codec::encode_probe_command(cmd, &arg_bytes)?;
    println!("[DRY-RUN] Probe frame for command 0x{:02x}: {:02x?}", cmd, &frame);

    if gate.confirm && gate.authorized(WriteGateLevel::Level1AddressingOnly) {
        let target_path = select_device_path(device)?;

        println!(
            "[Level 1 Authorized] Transmitting probe command to {}...",
            target_path.display()
        );
        let mut dev =
            Device::open_with_gate_level(&target_path, WriteGateLevel::Level1AddressingOnly)?;
        match dev.write_feature(&frame) {
            Ok(()) => {
                println!("SET_REPORT (Feature): SUCCESS");
                std::thread::sleep(std::time::Duration::from_millis(2));
                let mut resp = [0u8; FEATURE_REPORT_LEN];
                match dev.read_feature(&mut resp) {
                    Ok(()) => {
                        println!("GET_REPORT (Feature): {:02x?}", &resp);
                        println!(
                            "Raw hex: {}",
                            resp.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")
                        );
                    }
                    Err(e) => {
                        println!("GET_REPORT (Feature) failed: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("SET_REPORT (Feature) failed: {}", e);
                return Err(e);
            }
        }
    } else {
        println!(
            "\nDry-run completed. To transmit probe command to hardware, specify --confirm AND --unsafe-addressing (Write-Gate Level 1)."
        );
    }
    Ok(())
}
