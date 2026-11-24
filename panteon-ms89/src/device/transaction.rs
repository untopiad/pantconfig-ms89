use crate::codec::{
    encode_config_unlock, encode_config_unlock_commit_bank, encode_config_unlock_final_commit,
};
use crate::model::{PackedMemLayout, PackedMemMode};
use crate::{PanteonError, FEATURE_REPORT_LEN};
use super::gate::WriteGateLevel;
use super::Transport;

/// A block write representing 8 banks (banks 0..7) of 16-bit register values at a single base address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockWrite {
    pub addr: u16,
    pub banks: [u16; 8],
}

impl BlockWrite {
    pub fn new(addr: u16, banks: [u16; 8]) -> Self {
        Self { addr, banks }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteTransactionPlan {
    pub bank: u8,
    pub addr: u16,
    pub data: u16,
    /// Known previous register value (ADR-001 Phase 1 Snapshot).
    /// Used by `Transport::rollback_transaction` (ADR-001 Phase 4 Rollback) to restore the
    /// register to its original state if any write step fails. If `None`, rollback is skipped.
    pub initial_data: Option<u16>,
    pub preamble_data_frame: [u8; FEATURE_REPORT_LEN],
    pub preamble_commit_bank_frame: [u8; FEATURE_REPORT_LEN],
    pub preamble_final_commit_frame: [u8; FEATURE_REPORT_LEN],
    pub data_frame: [u8; FEATURE_REPORT_LEN],
    pub commit_bank_frame: [u8; FEATURE_REPORT_LEN],
    pub final_commit_frame: [u8; FEATURE_REPORT_LEN],
    pub expected_ack: [u8; 8],
}

impl WriteTransactionPlan {
    /// Create transaction plan for modifying register (bank, addr) to data without known initial state
    pub fn new(bank: u8, addr: u16, data: u16) -> Self {
        Self::with_initial_data(bank, addr, data, None)
    }

    /// Create transaction plan with known initial state for rollback (ADR-001 Section 2.2).
    ///
    /// When `initial_data` is `Some(val)`, `Transport::rollback_transaction` will issue a
    /// compensating sequence to restore `(bank, addr)` back to `val` if Phase 2 fails.
    pub fn with_initial_data(bank: u8, addr: u16, data: u16, initial_data: Option<u16>) -> Self {
        let preamble_data_frame = encode_config_unlock();
        let preamble_commit_bank_frame = encode_config_unlock_commit_bank();
        let preamble_final_commit_frame = encode_config_unlock_final_commit();

        // 1. Data write: mode 0x03
        let data_frame = PackedMemLayout::new(
            PackedMemMode::Write as u8,
            bank,
            addr,
            data,
        )
        .pack();

        // 2. Commit bank: mode 0x09 (bank 0x00, addr, data)
        let commit_bank_frame = PackedMemLayout::new(
            PackedMemMode::CommitBank as u8,
            0x00,
            addr,
            data,
        )
        .pack();

        // 3. Final commit: mode 0x00 (bank 0x00, addr, data 0x0000)
        let final_commit_frame = PackedMemLayout::new(
            PackedMemMode::FinalCommit as u8,
            0x00,
            addr,
            0x0000,
        )
        .pack();

        Self {
            bank,
            addr,
            data,
            initial_data,
            preamble_data_frame,
            preamble_commit_bank_frame,
            preamble_final_commit_frame,
            data_frame,
            commit_bank_frame,
            final_commit_frame,
            expected_ack: [0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        }
    }

    /// Preamble frames (3 frames: P1 Data Write, P2 Commit Bank, P3 Final Commit)
    pub fn preamble_frames(&self) -> [&[u8; FEATURE_REPORT_LEN]; 3] {
        [
            &self.preamble_data_frame,
            &self.preamble_commit_bank_frame,
            &self.preamble_final_commit_frame,
        ]
    }

    /// Main transaction frames (3 frames: Write, Commit Bank, Final Commit)
    pub fn main_frames(&self) -> [&[u8; FEATURE_REPORT_LEN]; 3] {
        [
            &self.data_frame,
            &self.commit_bank_frame,
            &self.final_commit_frame,
        ]
    }

    /// Formats the transaction frames as hex strings for CLI dry-run and logging
    pub fn log_frames(&self) -> Vec<String> {
        vec![
            format!("Preamble P1 (Data Write 0x0001): {:02x?}", &self.preamble_data_frame),
            format!("Preamble P2 (Commit Bank 0x0001): {:02x?}", &self.preamble_commit_bank_frame),
            format!("Preamble P3 (Final Commit 0x0001): {:02x?}", &self.preamble_final_commit_frame),
            format!("Phase 1 (Data Write mode 0x03): {:02x?}", &self.data_frame),
            format!("Phase 2 (Commit Bank mode 0x09): {:02x?}", &self.commit_bank_frame),
            format!("Phase 3 (Final Commit mode 0x00): {:02x?}", &self.final_commit_frame),
            format!("Phase 4 (Expected EP82 ACK): {:02x?}", &self.expected_ack),
        ]
    }
}

/// Execute or simulate a write transaction with preamble unlock (ADR-001)
pub fn execute_transaction<T: Transport + ?Sized>(
    transport: &mut T,
    plan: &WriteTransactionPlan,
    dry_run: bool,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    if dry_run {
        return Ok(());
    }
    if transport.write_gate_level() < WriteGateLevel::Level2ConfigurationWrite {
        return Err(PanteonError::WriteDisabled);
    }
    if !transport.device_identity().protocol_verified {
        return Err(PanteonError::ProtocolDetailed {
            context: "write_transaction",
            detail: format!(
                "protocol unverified for PID 0x{:04x} — run verify-protocol first",
                transport.device_identity().pid
            ),
        });
    }

    execute_transaction_unchecked(transport, plan, settle_ms)
}

/// Execute a multi-phase configuration write transaction without checking identity verification.
///
/// # Safety / Warning
/// Bypasses the Level 2 protocol verification check. Call ONLY from `verify-protocol`
/// after explicit user confirmation (`--confirm --write`).
pub(crate) fn execute_transaction_unchecked<T: Transport + ?Sized>(
    transport: &mut T,
    plan: &WriteTransactionPlan,
    settle_ms: u64,
) -> Result<(), PanteonError> {
    if transport.write_gate_level() < WriteGateLevel::Level2ConfigurationWrite {
        return Err(PanteonError::WriteDisabled);
    }

    // Phase P1: Preamble Data Write
    if let Err(e) = transport.raw_write_feature(&plan.preamble_data_frame) {
        return Err(PanteonError::Io(format!("frame P1 failed: {}", e)));
    }
    if settle_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(settle_ms));
    }

    // Phase P2: Preamble Commit Bank
    if let Err(e) = transport.raw_write_feature(&plan.preamble_commit_bank_frame) {
        return Err(PanteonError::Io(format!("frame P2 failed: {}", e)));
    }
    if settle_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(settle_ms));
    }

    // Phase P3: Preamble Final Commit
    if let Err(e) = transport.raw_write_feature(&plan.preamble_final_commit_frame) {
        return Err(PanteonError::Io(format!("frame P3 failed: {}", e)));
    }
    if settle_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(settle_ms));
    }

    // Phase W1: Main Data Write
    if let Err(e) = transport.raw_write_feature(&plan.data_frame) {
        rollback_transaction(transport, plan);
        return Err(PanteonError::Io(format!("frame W1 failed: {}", e)));
    }
    if settle_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(settle_ms));
    }

    // Phase W2: Main Commit Bank
    if let Err(e) = transport.raw_write_feature(&plan.commit_bank_frame) {
        rollback_transaction(transport, plan);
        return Err(PanteonError::Io(format!("frame W2 failed: {}", e)));
    }
    if settle_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(settle_ms));
    }

    // Phase W3: Main Final Commit
    if let Err(e) = transport.raw_write_feature(&plan.final_commit_frame) {
        rollback_transaction(transport, plan);
        return Err(PanteonError::Io(format!("frame W3 failed: {}", e)));
    }

    Ok(())
}

/// Rollback main configuration sequence upon error (ADR-001 Section 2.2 Phase 4).
///
/// If `plan.initial_data` is `Some(old_data)`, this issues a compensating 3-frame write
/// sequence restoring the register back to `old_data`.
pub fn rollback_transaction<T: Transport + ?Sized>(transport: &mut T, plan: &WriteTransactionPlan) {
    if let Some(old_data) = plan.initial_data {
        let rollback_plan = WriteTransactionPlan::new(plan.bank, plan.addr, old_data);
        let _ = transport.raw_write_feature(&rollback_plan.data_frame);
        let _ = transport.raw_write_feature(&rollback_plan.commit_bank_frame);
        let _ = transport.raw_write_feature(&rollback_plan.final_commit_frame);
    }
}
