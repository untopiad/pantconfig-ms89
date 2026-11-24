use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub use super::args::{parse_hex_color_arg, parse_hex_or_dec_u16, parse_hex_or_dec_u8, GateArgs};

#[derive(Parser)]
#[command(name = "panteon-ms89", author = "Senior Rust Engineer", version = "0.1.2", about = "Linux userspace configurator for PANTEON MS89", long_about = "Linux configurator for PANTEON MS89.\nSAFETY NOTICE: Hardware writes are strictly disabled until protocol verification via live traffic capture.\nOnly passive discovery and explicit read-only feature report probes are permitted.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List matching PANTEON MS89 hidraw devices in /dev
    ListDevices,
    /// Diagnose udev-rule installation and device-node access (read-only)
    ///
    /// Prints the udev rule state, hidraw access and USB-reset access. If
    /// something is missing, prints a ready-to-run installation command.
    /// Exit code: 0 when access is fully configured, 1 otherwise.
    CheckAccess {
        /// Print only the udev rule contents (for pipes) and exit 0
        #[arg(long)]
        print_rule: bool,
    },
    /// Probe a PANTEON MS89 device and read its handshake status
    Probe {
        /// Allow reading feature report from device (read-only)
        #[arg(long, short)]
        read: bool,
        /// Path to specific hidraw device node (e.g. /dev/hidraw0)
        #[arg(long, short)]
        device: Option<PathBuf>,
    },
    /// Export mouse configuration profile to TOML
    Export {
        /// Output file path (prints to stdout if omitted)
        #[arg(long, short)]
        output: Option<PathBuf>,
        /// Profile ID 0..3 (defaults to 0)
        #[arg(long, short, default_value_t = 0)]
        profile: u8,
    },
    /// Import and validate mouse configuration profile from TOML file
    Import {
        /// Path to TOML profile file
        path: PathBuf,
    },
    /// Dump full register ledger state (requires Level 1 addressing write)
    DumpState {
        /// Path to specific hidraw device node
        #[arg(long, short)]
        device: Option<PathBuf>,
        /// Path to shadow ledger state file (default: $XDG_STATE_HOME/panteon-ms89/ledger.toml)
        #[arg(long)]
        state: Option<PathBuf>,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Experimental read-probe to test candidate memory addressing sequences
    ExperimentalReadProbe {
        /// Path to specific hidraw device node
        #[arg(long, short)]
        device: Option<PathBuf>,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Experimental readback of arbitrary memory register (mode 0x08, experimental, protocol-probed)
    ExperimentalRead {
        /// Bank number (0..7)
        #[arg(value_parser = parse_hex_or_dec_u8)]
        bank: u8,
        /// Register address (hex e.g. 0x0018 or decimal)
        #[arg(value_parser = parse_hex_or_dec_u16)]
        addr: u16,
        /// Path to specific hidraw device node
        #[arg(long, short)]
        device: Option<PathBuf>,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Special experimental readback mode for POLLING_RATE register (Bank 6, Addr 0x0018) with ground truth matching
    ExperimentalReadPolling {
        /// Path to specific hidraw device node
        #[arg(long, short)]
        device: Option<PathBuf>,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Linear memory read sweep by candidate mode 0x05 protocol (experimental, protocol-probed)
    ExperimentalReadSweep {
        /// Base linear address to start reading from (default 0)
        #[arg(long, default_value = "0", value_parser = parse_hex_or_dec_u16)]
        base: u16,
        /// Number of consecutive addresses to sweep (default 1024)
        #[arg(long, default_value_t = 1024)]
        count: u16,
        /// Path to specific hidraw device node
        #[arg(long, short)]
        device: Option<PathBuf>,
        /// Destination path for raw JSON output (required)
        #[arg(long)]
        output: PathBuf,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Read consecutive bytes from a specific bank using bank-addressing read protocol
    ExperimentalReadBank {
        /// Target bank index (0..7)
        #[arg(long, short, value_parser = parse_hex_or_dec_u8)]
        bank: u8,
        /// Memory address within bank (e.g. 0x0018 or 24)
        #[arg(long, short, value_parser = parse_hex_or_dec_u16)]
        addr: u16,
        /// Number of consecutive bytes to read (1..256, defaults to 4)
        #[arg(long, short, default_value_t = 4)]
        count: usize,
        /// Path to target hidraw device node (defaults to auto-discovered interface 1)
        #[arg(long, short)]
        device: Option<PathBuf>,
        /// Optional output file for JSON payload
        #[arg(long, short)]
        output: Option<PathBuf>,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Read and dump 32-byte descriptor page (bank 0, address 0x0000)
    DumpDescriptor {
        /// Path to specific hidraw device node (e.g. /dev/hidraw0)
        #[arg(long, short)]
        device: Option<PathBuf>,
        /// Output results as JSON
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Configure mouse polling rate (125, 250, 500, or 1000 Hz)
    SetPolling {
        /// Target polling rate in Hz (125, 250, 500, or 1000)
        rate: u16,
        /// Path to specific hidraw device node
        #[arg(long, short)]
        device: Option<PathBuf>,
        /// Run in dry-run mode (default behavior)
        #[arg(long)]
        dry_run: bool,
        /// Pause between frames in milliseconds (default 0)
        #[arg(long, default_value_t = 0)]
        settle_ms: u64,
        /// Commit state to non-volatile SPI Flash (mode 0x10 flush frame)
        #[arg(long)]
        commit_flash: bool,
        /// Execute GET_REPORT handshake prior to write sequence
        #[arg(long)]
        get_first: bool,
        /// Path to shadow ledger state file (default: $XDG_STATE_HOME/panteon-ms89/ledger.toml)
        #[arg(long)]
        state: Option<PathBuf>,
        /// Disable recording in shadow ledger
        #[arg(long)]
        no_ledger: bool,
        /// Explicit active DPI slot (1..=6) override for block 0x0018
        #[arg(long, value_parser = clap::value_parser!(u8))]
        active_slot: Option<u8>,
        /// Skip the post-write USB soft-reset (device stays enumerated; R10.1 live-apply experiment)
        #[arg(long)]
        no_soft_reset: bool,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Configure active DPI slot (1..=6)
    #[command(alias = "set-active-dpi-slot")]
    SetActiveDpi {
        /// Target DPI slot (1..=6)
        slot: u8,
        /// Path to specific hidraw device node
        #[arg(long, short)]
        device: Option<PathBuf>,
        /// Run in dry-run mode (default behavior)
        #[arg(long)]
        dry_run: bool,
        /// Pause between frames in milliseconds (default 0)
        #[arg(long, default_value_t = 0)]
        settle_ms: u64,
        /// Commit state to non-volatile SPI Flash (mode 0x10 flush frame)
        #[arg(long)]
        commit_flash: bool,
        /// Path to shadow ledger state file (default: $XDG_STATE_HOME/panteon-ms89/ledger.toml)
        #[arg(long)]
        state: Option<PathBuf>,
        /// Disable recording in shadow ledger
        #[arg(long)]
        no_ledger: bool,
        /// Skip the post-write USB soft-reset (device stays enumerated; R10.1 live-apply experiment)
        #[arg(long)]
        no_soft_reset: bool,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Configure fire button delay in milliseconds (8..=300 ms)
    #[command(alias = "set-fire")]
    SetFireDelay {
        /// Delay in milliseconds (8..=300)
        delay_ms: u16,
        /// Path to specific hidraw device node
        #[arg(long, short)]
        device: Option<PathBuf>,
        /// Run in dry-run mode (default behavior)
        #[arg(long)]
        dry_run: bool,
        /// Pause between frames in milliseconds (default 0)
        #[arg(long, default_value_t = 0)]
        settle_ms: u64,
        /// Commit state to non-volatile SPI Flash (mode 0x10 flush frame)
        #[arg(long)]
        commit_flash: bool,
        /// Path to shadow ledger state file (default: $XDG_STATE_HOME/panteon-ms89/ledger.toml)
        #[arg(long)]
        state: Option<PathBuf>,
        /// Disable recording in shadow ledger
        #[arg(long)]
        no_ledger: bool,
        /// Explicit active DPI slot (1..=6) override for block 0x0018
        #[arg(long, value_parser = clap::value_parser!(u8))]
        active_slot: Option<u8>,
        /// Skip the post-write USB soft-reset (device stays enumerated; R10.1 live-apply experiment)
        #[arg(long)]
        no_soft_reset: bool,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Configure LED brightness and effect parameters
    #[command(alias = "set-rgb")]
    SetLed {
        /// Global LED brightness level (0..=8; 0 equals 8 = 100%, 1 = off, 2=25%, 4=50%, 6=75%, 8=100%)
        #[arg(long, value_parser = clap::value_parser!(u8))]
        brightness: Option<u8>,
        /// Optional LED effect index (0..=13)
        #[arg(long)]
        effect: Option<u8>,
        /// Optional LED speed (0..=5)
        #[arg(long)]
        speed: Option<u8>,
        /// Path to specific hidraw device node
        #[arg(long, short)]
        device: Option<PathBuf>,
        /// Run in dry-run mode (default behavior)
        #[arg(long)]
        dry_run: bool,
        /// Pause between frames in milliseconds (default 0)
        #[arg(long, default_value_t = 0)]
        settle_ms: u64,
        /// Commit state to non-volatile SPI Flash (mode 0x10 flush frame)
        #[arg(long)]
        commit_flash: bool,
        /// Path to shadow ledger state file (default: $XDG_STATE_HOME/panteon-ms89/ledger.toml)
        #[arg(long)]
        state: Option<PathBuf>,
        /// Disable recording in shadow ledger
        #[arg(long)]
        no_ledger: bool,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Configure all 6 DPI slots simultaneously
    SetDpiSlots {
        slot1: u32,
        slot2: u32,
        slot3: u32,
        slot4: u32,
        slot5: u32,
        slot6: u32,
        /// Optional RGB colors for all 6 slots (#rrggbb or rrggbb)
        #[arg(long, num_args = 6, value_names = ["C1", "C2", "C3", "C4", "C5", "C6"], value_parser = parse_hex_color_arg)]
        colors: Option<Vec<(u8, u8, u8)>>,
        /// Optional global LED brightness level (0..=8; 0 equals 8 = 100%, 1 = off, 2=25%, 4=50%, 6=75%, 8=100%)
        #[arg(long, value_parser = clap::value_parser!(u8))]
        brightness: Option<u8>,
        /// Execute writes using block-oriented session (8 banks per block commit, single preamble)
        #[arg(long)]
        block_session: bool,
        #[arg(long, short)]
        device: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 0)]
        settle_ms: u64,
        #[arg(long)]
        commit_flash: bool,
        #[arg(long)]
        state: Option<PathBuf>,
        #[arg(long)]
        no_ledger: bool,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Configure a single DPI slot (1..=6) with optional color
    SetDpiSlot {
        slot: u8,
        dpi: u32,
        #[arg(long)]
        color: Option<String>,
        #[arg(long, short)]
        device: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, default_value_t = 0)]
        settle_ms: u64,
        #[arg(long)]
        commit_flash: bool,
        #[arg(long)]
        state: Option<PathBuf>,
        #[arg(long)]
        no_ledger: bool,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Push full 867-frame Windows Apply state to device based on shadow ledger overlay (M4.2)
    PushState {
        #[arg(long, default_value_t = true)]
        full: bool,
        #[arg(long, short)]
        device: Option<PathBuf>,
        #[arg(long)]
        state: Option<PathBuf>,
        #[arg(long)]
        settle_override: Option<u64>,
        #[arg(long)]
        force_pid_mismatch: bool,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Verify device protocol identity with read-only probes (M2.7)
    VerifyProtocol {
        /// Path to specific hidraw device node
        #[arg(long, short)]
        device: Option<PathBuf>,
    },
    /// Remove persistent protocol verification marker(s) (M4-hf)
    #[command(alias = "forget_verification")]
    ForgetVerification {
        /// Optional PID to forget (hex or decimal, e.g. 0x1e01; if omitted, all markers are cleared)
        #[arg(long, value_parser = parse_hex_or_dec_u16)]
        pid: Option<u16>,
        /// Optional path to custom verified.toml marker file
        #[arg(long)]
        state: Option<PathBuf>,
    },
    /// Send raw feature report frame (bisection / hardware debugging tool)
    RawFrame {
        /// Hex string of the frame to send (e.g. "07 18 03 00 01 00 03 00" or "0718030001000300")
        hex_data: String,
        /// Path to specific hidraw device node
        #[arg(long, short)]
        device: Option<PathBuf>,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Probe firmware command with raw arguments under Write-Gate Level 1 (M2.3)
    ProbeCommand {
        /// Command byte (hex or dec, e.g. 0x0b, 0x13, 0x15, 0x16, 0x20)
        #[arg(value_parser = parse_hex_or_dec_u8)]
        cmd: u8,
        /// Optional argument bytes in hex (e.g. "00 80 00 40" or "00800040", up to 6 bytes)
        #[arg(default_value = "")]
        args: String,
        /// Path to specific hidraw device node
        #[arg(long, short)]
        device: Option<PathBuf>,
        #[command(flatten)]
        gate: GateArgs,
    },
    /// Import Windows driver capture (pcapng) into a state snapshot TOML
    CaptureImport {
        /// Path to pcapng capture file
        pcapng: PathBuf,
        /// Label for this snapshot
        #[arg(long)]
        label: String,
        /// Source device PID at capture time (e.g. 0x1440 or 0x1e01)
        #[arg(long, value_parser = parse_hex_or_dec_u16)]
        pid: u16,
        /// Output path for snapshot TOML file (default: snapshot.toml)
        #[arg(long, short)]
        out: Option<PathBuf>,
    },
    /// Replay state snapshot frames directly to device
    RestoreState {
        /// Path to snapshot TOML file
        snapshot: PathBuf,
        /// Path to specific hidraw device node (e.g. /dev/hidraw0)
        #[arg(long, short)]
        device: Option<PathBuf>,
        /// Interactive step-by-step confirmation before sending each frame
        #[arg(long)]
        step: bool,
        /// Inter-frame delay override in milliseconds
        #[arg(long)]
        settle_override: Option<u64>,
        /// Force restore even if snapshot source PID differs from device PID
        #[arg(long)]
        force_pid_mismatch: bool,
        #[command(flatten)]
        gate: GateArgs,
    },
}
