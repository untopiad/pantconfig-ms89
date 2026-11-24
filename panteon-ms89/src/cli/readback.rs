use std::path::PathBuf;

use panteon_ms89::codec::{decode_descriptor_page, encode_read_request, READ_REQUEST_MODE};
use panteon_ms89::device::{select_device_path, Device, WriteGateLevel};
use panteon_ms89::model::registers;
use panteon_ms89::PanteonError;

use super::definition::GateArgs;
use super::render::{
    fmt_hex_slice, print_dry_run_notice, print_gate_authorized, print_unsafe_addressing_error,
};

pub fn run_experimental_read(
    bank: u8,
    addr: u16,
    device: Option<PathBuf>,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let req_frame = encode_read_request(bank, addr);
    println!(
        "[DRY-RUN] Experimental read request frame for Bank {}, Addr 0x{:04x} (mode 0x08):",
        bank, addr
    );
    println!("  SET_REPORT frame: {:02x?}", &req_frame);

    if gate.authorized(WriteGateLevel::Level1AddressingOnly) {
        let target_path = select_device_path(device)?;
        print_gate_authorized(WriteGateLevel::Level1AddressingOnly, &target_path);
        let mut dev = Device::open_with_gate_level(
            &target_path,
            WriteGateLevel::Level1AddressingOnly,
        )?;
        println!(
            "Transmitting mode 0x08 read request frame for (Bank {}, Addr 0x{:04x})...",
            bank, addr
        );
        let raw_response = dev.experimental_read_register(bank, addr)?;
        println!("Raw response (7 bytes): {:02x?}", &raw_response);
        println!("Raw hex: {}", fmt_hex_slice(&raw_response));
    } else if gate.confirm && !gate.unsafe_addressing {
        print_unsafe_addressing_error("experimental-read");
        return Err(PanteonError::WriteDisabled);
    } else {
        print_dry_run_notice("experimental read");
    }
    Ok(())
}

pub fn run_experimental_read_polling(
    device: Option<PathBuf>,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let bank = registers::POLLING_RATE.0;
    let addr = registers::POLLING_RATE.1;
    let req_frame = encode_read_request(bank, addr);
    println!(
        "[DRY-RUN] Experimental read request for POLLING_RATE (Bank {}, Addr 0x{:04x}, mode 0x{:02x}):",
        bank, addr, READ_REQUEST_MODE
    );
    println!("  SET_REPORT frame: {:02x?}", &req_frame);

    if gate.authorized(WriteGateLevel::Level1AddressingOnly) {
        let target_path = select_device_path(device)?;
        print_gate_authorized(WriteGateLevel::Level1AddressingOnly, &target_path);
        let mut dev = Device::open_with_gate_level(
            &target_path,
            WriteGateLevel::Level1AddressingOnly,
        )?;
        println!(
            "Transmitting mode 0x{:02x} read request frame for POLLING_RATE (Bank {}, Addr 0x{:04x})...",
            READ_REQUEST_MODE, bank, addr
        );
        let raw_response = dev.experimental_read_register(bank, addr)?;
        println!("Raw response (7 bytes): {:02x?}", &raw_response);
        println!("Raw hex: {}", fmt_hex_slice(&raw_response));

        println!("\nGround truth matching against confirmed polling rate register codes:");
        let mut match_found = false;
        for i in 0..6 {
            let word_le = u16::from_le_bytes([raw_response[i], raw_response[i + 1]]);
            let label = match word_le {
                0x0700 => Some("1000 Hz (0x0700)"),
                0x0701 => Some("500 Hz (0x0701)"),
                0x0707 => Some("125 Hz (0x0707)"),
                0x0703 => Some("250 Hz (0x0703 - hypothesis)"),
                _ => None,
            };
            if let Some(lbl) = label {
                println!("  -> MATCH at byte offset {}..{}: 0x{:04x} = {}", i, i + 2, word_le, lbl);
                match_found = true;
            }
        }
        if !match_found {
            println!("  -> No direct 16-bit LE match for {{0x0700: 1000Hz, 0x0701: 500Hz, 0x0707: 125Hz}} in payload.");
        }
    } else if gate.confirm && !gate.unsafe_addressing {
        print_unsafe_addressing_error("experimental-read-polling");
        return Err(PanteonError::WriteDisabled);
    } else {
        print_dry_run_notice("experimental read");
    }
    Ok(())
}

pub fn run_dump_descriptor(
    device: Option<PathBuf>,
    json: bool,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let target_display = device
        .clone()
        .unwrap_or_else(|| PathBuf::from("auto-discovered"));

    if !gate.authorized(WriteGateLevel::Level1AddressingOnly) {
        if gate.confirm && !gate.unsafe_addressing {
            print_unsafe_addressing_error("dump-descriptor");
            return Err(PanteonError::WriteDisabled);
        } else {
            println!("[DRY-RUN] Dump descriptor plan (32 bytes at bank 0, addr 0x0000):");
            println!("  Target device: {}", target_display.display());
            println!(
                "  Read protocol: Bank 0, Base 0x0000, Count 32, Delay {:?}",
                Device::READ_PROTOCOL_DELAY
            );
            print_dry_run_notice("dump-descriptor");
            return Ok(());
        }
    }

    let target_path = select_device_path(device)?;
    if !json {
        println!();
        print_gate_authorized(WriteGateLevel::Level1AddressingOnly, &target_path);
    }
    let mut dev = Device::open_with_gate_level(&target_path, WriteGateLevel::Level1AddressingOnly)?;
    let raw_bytes = dev.experimental_read(0, 0, 32, Device::READ_PROTOCOL_DELAY)?;
    let descriptor = decode_descriptor_page(&raw_bytes)?;

    if json {
        print!("{}", descriptor.to_json_string());
    } else {
        print!("{}", descriptor.format_table());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use crate::cli::definition::{Cli, Commands};

    #[test]
    fn test_dump_descriptor_cli_parsing() {
        let cli = Cli::try_parse_from(["panteon-ms89", "dump-descriptor", "--json"]).unwrap();
        match cli.command {
            Commands::DumpDescriptor { json, gate, .. } => {
                assert!(json);
                assert!(!gate.authorized(WriteGateLevel::Level1AddressingOnly));
            }
            _ => panic!("Expected DumpDescriptor"),
        }

        let cli_l1 = Cli::try_parse_from([
            "panteon-ms89",
            "dump-descriptor",
            "--confirm",
            "--unsafe-addressing",
        ])
        .unwrap();
        match cli_l1.command {
            Commands::DumpDescriptor { json, gate, .. } => {
                assert!(!json);
                assert!(gate.authorized(WriteGateLevel::Level1AddressingOnly));
            }
            _ => panic!("Expected DumpDescriptor"),
        }
    }
}
