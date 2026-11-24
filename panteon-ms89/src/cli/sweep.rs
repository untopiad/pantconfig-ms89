use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use panteon_ms89::codec::{
    encode_read_bank_arm_finalize, encode_read_bank_frame, encode_read_bank_prep_frame,
    encode_read_reset_frame, encode_read_sweep_frame,
};
use panteon_ms89::device::{select_device_path, Device, WriteGateLevel};
use panteon_ms89::PanteonError;

use super::definition::GateArgs;
use super::render::{
    fmt_hex_slice, print_dry_run_notice, print_gate_authorized, print_unsafe_addressing_error,
};

/// Typed dump structure for memory sweep results
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweepDump {
    pub records: BTreeMap<u16, u8>,
}

impl SweepDump {
    pub fn to_json_string(&self) -> String {
        let mut json_str = String::from("{\n");
        let total = self.records.len();
        for (idx, (addr, val)) in self.records.iter().enumerate() {
            let comma = if idx + 1 == total { "" } else { "," };
            json_str.push_str(&format!("  \"0x{:04x}\": \"0x{:02x}\"{}\n", addr, val, comma));
        }
        json_str.push_str("}\n");
        json_str
    }
}

/// Typed dump structure for bank read results
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankReadDump {
    pub bank: u8,
    pub addr: u16,
    pub bytes: Vec<u8>,
}

impl BankReadDump {
    pub fn to_json_string(&self) -> String {
        let byte_strs: Vec<String> = self.bytes.iter().map(|b| format!("\"0x{:02x}\"", b)).collect();
        format!(
            "{{\n  \"bank\": {},\n  \"addr\": {},\n  \"addr_hex\": \"0x{:04x}\",\n  \"count\": {},\n  \"bytes\": [{}]\n}}\n",
            self.bank, self.addr, self.addr, self.bytes.len(), byte_strs.join(", ")
        )
    }
}

pub fn run_experimental_read_sweep(
    base: u16,
    count: u16,
    device: Option<PathBuf>,
    output: PathBuf,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let first_addr = base;
    let last_addr = base.saturating_add(count.saturating_sub(1));
    let first_frame = encode_read_sweep_frame(0, first_addr, 0);
    let last_frame = encode_read_sweep_frame(0, last_addr, 0);
    let target_display = device
        .as_deref()
        .unwrap_or(Path::new("auto-discovered PANTEON MS89"));

    println!("[DRY-RUN] Experimental read sweep plan (mode 0x05):");
    println!("  Target device: {}", target_display.display());
    println!("  Base address: {} (0x{:04x})", base, base);
    println!("  Count: {} bytes", count);
    println!("  Address range: 0x{:04x}..=0x{:04x}", first_addr, last_addr);
    println!("  Output JSON path: {}", output.display());
    println!("  First SET frame (0x{:04x}): {:02x?}", first_addr, &first_frame);
    println!("  Last SET frame (0x{:04x}): {:02x?}", last_addr, &last_frame);
    println!("  Protocol: 1-byte read chunk per address (count=1), Sleep(2ms), GET_REPORT");

    if gate.authorized(WriteGateLevel::Level1AddressingOnly) {
        let target_path = select_device_path(device)?;
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|e| PanteonError::Io(e.to_string()))?;
        }

        println!();
        print_gate_authorized(WriteGateLevel::Level1AddressingOnly, &target_path);
        let mut dev = Device::open_with_gate_level(&target_path, WriteGateLevel::Level1AddressingOnly)?;
        println!(
            "Starting linear read sweep of {} addresses (0x{:04x}..=0x{:04x})...",
            count, first_addr, last_addr
        );
        let mut sweep_results = BTreeMap::new();
        let count_usize = count as usize;
        let chunk_size = 256usize;
        let mut done = 0usize;
        while done < count_usize {
            let n = (count_usize - done).min(chunk_size);
            let chunk_base = base + done as u16;
            let bytes = dev.experimental_read_chunk(chunk_base, n)?;
            for (idx, b) in bytes.iter().enumerate() {
                sweep_results.insert(chunk_base + idx as u16, *b);
            }
            done += n;
            println!("Progress: {}/{} addresses read (chunk base 0x{:04x}, {} bytes)...", done, count, chunk_base, n);
        }

        let dump = SweepDump { records: sweep_results };
        let json_str = dump.to_json_string();
        fs::write(&output, json_str).map_err(|e| PanteonError::Io(e.to_string()))?;
        println!(
            "Sweep complete. Successfully wrote {} records to {}",
            dump.records.len(), output.display()
        );
    } else if gate.confirm && !gate.unsafe_addressing {
        print_unsafe_addressing_error("experimental-read-sweep");
        return Err(PanteonError::WriteDisabled);
    } else {
        print_dry_run_notice("sweep");
    }
    Ok(())
}

pub fn run_experimental_read_bank(
    bank: u8,
    addr: u16,
    count: usize,
    device: Option<PathBuf>,
    output: Option<PathBuf>,
    gate: GateArgs,
) -> Result<(), PanteonError> {
    let target_display = device
        .as_deref()
        .unwrap_or(Path::new("auto-discovered PANTEON MS89"));

    let count_minus_1 = count.saturating_sub(1).min(255) as u8;
    let last_offset = count.saturating_sub(1) as u16;
    let first_prep = encode_read_bank_prep_frame(bank, addr, count_minus_1);
    let last_prep = encode_read_bank_prep_frame(bank, addr + last_offset, count_minus_1);
    let arm_frame = encode_read_bank_arm_finalize(bank, addr);
    let first_read = encode_read_bank_frame(bank, addr, count_minus_1);
    let last_read = encode_read_bank_frame(bank, addr + last_offset, count_minus_1);
    let reset_frame = encode_read_reset_frame();

    println!("[DRY-RUN] Experimental bank read plan (mode 0x05 / mode 0x09 bank addressing):");
    println!("  Target device: {}", target_display.display());
    println!("  Bank:          {} (0x{:02x})", bank, bank);
    println!("  Base address:  0x{:04x} ({})", addr, addr);
    println!("  Count:         {} bytes", count);
    println!("  Address range: 0x{:04x}..=0x{:04x}", addr, addr + last_offset);
    println!("  First PREP frame (mode 0x09): {:02x?}", &first_prep);
    println!("  Last PREP frame  (mode 0x09): {:02x?}", &last_prep);
    println!("  ARM frame        (mode 0x00): {:02x?}", &arm_frame);
    println!("  First READ frame (mode 0x05): {:02x?}", &first_read);
    println!("  Last READ frame  (mode 0x05): {:02x?}", &last_read);
    println!("  END reset frame  (mode 0x00): {:02x?}", &reset_frame);

    if gate.authorized(WriteGateLevel::Level1AddressingOnly) {
        let target_path = select_device_path(device)?;
        println!();
        print_gate_authorized(WriteGateLevel::Level1AddressingOnly, &target_path);
        let mut dev = Device::open_with_gate_level(&target_path, WriteGateLevel::Level1AddressingOnly)?;
        println!(
            "Executing bank read: Bank {}, Addr 0x{:04x}, Count {}...",
            bank, addr, count
        );
        let bytes = dev.experimental_read_bank(bank, addr, count)?;
        let hex_str = fmt_hex_slice(&bytes);
        println!("Read {} bytes: {}", bytes.len(), hex_str);
        for (i, b) in bytes.iter().enumerate() {
            println!("  [0x{:04x} + {:2}]: 0x{:02x} ({:3})", addr + i as u16, i, b, b);
        }

        let dump = BankReadDump { bank, addr, bytes };
        let json_obj = dump.to_json_string();
        println!("\nJSON Output:\n{}", json_obj);

        if let Some(out_path) = output {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| PanteonError::Io(e.to_string()))?;
            }
            fs::write(&out_path, &json_obj).map_err(|e| PanteonError::Io(e.to_string()))?;
            println!("Saved output to {}", out_path.display());
        }
    } else if gate.confirm && !gate.unsafe_addressing {
        print_unsafe_addressing_error("experimental-read-bank");
        return Err(PanteonError::WriteDisabled);
    } else {
        print_dry_run_notice("bank read");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_typed_dump_json_serialization() {
        let mut records = BTreeMap::new();
        records.insert(0x0010, 0x5a);
        records.insert(0x0011, 0x3c);
        let sweep = SweepDump { records };
        let json = sweep.to_json_string();
        assert!(json.contains("\"0x0010\": \"0x5a\""));
        assert!(json.contains("\"0x0011\": \"0x3c\""));

        let bank_dump = BankReadDump {
            bank: 6,
            addr: 0x0018,
            bytes: vec![0x00, 0x07],
        };
        let b_json = bank_dump.to_json_string();
        assert!(b_json.contains("\"bank\": 6"));
        assert!(b_json.contains("\"addr_hex\": \"0x0018\""));
        assert!(b_json.contains("\"bytes\": [\"0x00\", \"0x07\"]"));
    }
}
