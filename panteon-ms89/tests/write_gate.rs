use std::time::Duration;

use panteon_ms89::codec::{
    encode_config_unlock, encode_config_unlock_commit_bank, encode_config_unlock_final_commit,
    encode_read_request, encode_read_request_0x13, HYPOTHETICAL_PAGE_BASE_PROBE,
};
use panteon_ms89::device::{Transport, WriteGateLevel, WriteTransactionPlan};
use panteon_ms89::model::{registers, DeviceIdentity, PollingRate, ShadowEntry, ShadowLedger};
use panteon_ms89::{PanteonError, FEATURE_REPORT_LEN, REPORT_ID};

struct MockGateTransport {
    gate_level: WriteGateLevel,
    identity: DeviceIdentity,
    raw_writes: Vec<[u8; FEATURE_REPORT_LEN]>,
    fail_on_write_index: Option<usize>,
}

impl MockGateTransport {
    fn new(gate_level: WriteGateLevel) -> Self {
        Self {
            gate_level,
            identity: DeviceIdentity::unknown(),
            raw_writes: Vec::new(),
            fail_on_write_index: None,
        }
    }

    fn set_identity(&mut self, identity: DeviceIdentity) {
        self.identity = identity;
    }

    fn with_identity(gate_level: WriteGateLevel, identity: DeviceIdentity) -> Self {
        Self {
            gate_level,
            identity,
            raw_writes: Vec::new(),
            fail_on_write_index: None,
        }
    }

    fn with_failure(gate_level: WriteGateLevel, fail_on_index: usize) -> Self {
        Self {
            gate_level,
            identity: DeviceIdentity::unknown(),
            raw_writes: Vec::new(),
            fail_on_write_index: Some(fail_on_index),
        }
    }

    fn check_persisted_marker(
        &mut self,
        report: &[u8; FEATURE_REPORT_LEN],
        marker_path: Option<&std::path::Path>,
    ) -> bool {
        panteon_ms89::device::check_marker_verification_report(&mut self.identity, report, marker_path)
    }
}

impl Transport for MockGateTransport {
    fn device_identity(&self) -> DeviceIdentity {
        self.identity
    }

    fn read_feature(&mut self, report: &mut [u8; FEATURE_REPORT_LEN]) -> Result<(), PanteonError> {
        report[0] = REPORT_ID;
        Ok(())
    }

    fn write_gate_level(&self) -> WriteGateLevel {
        self.gate_level
    }

    fn set_write_gate_level(&mut self, level: WriteGateLevel) {
        self.gate_level = level;
    }

    fn raw_write_feature(&mut self, report: &[u8; FEATURE_REPORT_LEN]) -> Result<(), PanteonError> {
        if let Some(idx) = self.fail_on_write_index {
            if self.raw_writes.len() == idx {
                return Err(PanteonError::Io("Simulated hardware write failure".to_string()));
            }
        }
        self.raw_writes.push(*report);
        Ok(())
    }
}

#[test]
fn write_gate_level0_blocks_all_writes() {
    let mut transport = MockGateTransport::new(WriteGateLevel::Level0ReadOnly);

    // Data write (mode 0x03) must be blocked
    let data_report = [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x00, 0x07];
    assert_eq!(transport.write_feature(&data_report), Err(PanteonError::WriteDisabled));

    // Preamble writes must also be blocked under Level 0
    assert_eq!(transport.write_feature(&encode_config_unlock()), Err(PanteonError::WriteDisabled));
    assert_eq!(transport.write_feature(&encode_config_unlock_commit_bank()), Err(PanteonError::WriteDisabled));
    assert_eq!(transport.write_feature(&encode_config_unlock_final_commit()), Err(PanteonError::WriteDisabled));

    // Addressing / probe packet (mode 0x01) must also be blocked under Level 0
    let addr_report = encode_read_request(0x06, 0x0018);
    assert_eq!(transport.write_feature(&addr_report), Err(PanteonError::WriteDisabled));

    // PageBase 0x13 probe must also be blocked under Level 0
    assert_eq!(
        transport.write_feature(&HYPOTHETICAL_PAGE_BASE_PROBE),
        Err(PanteonError::WriteDisabled)
    );

    assert!(transport.raw_writes.is_empty());
}

#[test]
fn write_gate_level1_allows_addressing_blocks_data_writes() {
    let mut transport = MockGateTransport::new(WriteGateLevel::Level1AddressingOnly);

    // Addressing packet (mode 0x01) must be ALLOWED under Level 1
    let addr_report = encode_read_request(0x00, 0x0018);
    assert!(transport.write_feature(&addr_report).is_ok());

    // PageBase 0x13 candidate probe must be ALLOWED under Level 1
    assert!(transport.write_feature(&HYPOTHETICAL_PAGE_BASE_PROBE).is_ok());

    // PageBase 0x13 dynamically encoded must be ALLOWED under Level 1
    let addr_13 = encode_read_request_0x13(0x0080, 0x0040);
    assert!(transport.write_feature(&addr_13).is_ok());

    // Data write (mode 0x03) must still be STRICTLY BLOCKED under Level 1
    let data_report = [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x00, 0x07];
    assert_eq!(transport.write_feature(&data_report), Err(PanteonError::WriteDisabled));

    // Only the 3 addressing packets reached raw_write_feature
    assert_eq!(transport.raw_writes.len(), 3);
    assert_eq!(transport.raw_writes[0], addr_report);
    assert_eq!(transport.raw_writes[1], HYPOTHETICAL_PAGE_BASE_PROBE);
    assert_eq!(transport.raw_writes[2], addr_13);
}

#[test]
fn write_gate_level1_blocks_preamble() {
    let mut transport = MockGateTransport::new(WriteGateLevel::Level1AddressingOnly);
    let plan = WriteTransactionPlan::new(
        registers::POLLING_RATE.0,
        registers::POLLING_RATE.1,
        PollingRate::Hz1000.to_register_code(),
    );

    // Direct write of preamble data frame must return WriteDisabled
    assert_eq!(
        transport.write_feature(&encode_config_unlock()),
        Err(PanteonError::WriteDisabled)
    );

    // Direct write of preamble commit bank frame must return WriteDisabled
    assert_eq!(
        transport.write_feature(&encode_config_unlock_commit_bank()),
        Err(PanteonError::WriteDisabled)
    );

    // Direct write of preamble final commit frame must return WriteDisabled
    assert_eq!(
        transport.write_feature(&encode_config_unlock_final_commit()),
        Err(PanteonError::WriteDisabled)
    );

    // execute_transaction under Level 1 must return WriteDisabled
    assert_eq!(
        transport.execute_transaction(&plan, false),
        Err(PanteonError::WriteDisabled)
    );

    // Arbitrary commit bank frame targeting CONFIG_UNLOCK (addr 0x0001, mode 0x09) with custom data must be blocked
    let arbitrary_commit_bank = [0x07, 0x18, 0x09, 0x00, 0x01, 0x00, 0x05, 0x00];
    assert_eq!(
        transport.write_feature(&arbitrary_commit_bank),
        Err(PanteonError::WriteDisabled)
    );

    // Arbitrary final commit frame targeting CONFIG_UNLOCK (addr 0x0001, mode 0x00) with custom data must be blocked
    let arbitrary_final_commit = [0x07, 0x18, 0x00, 0x00, 0x01, 0x00, 0x07, 0x00];
    assert_eq!(
        transport.write_feature(&arbitrary_final_commit),
        Err(PanteonError::WriteDisabled)
    );

    // Read request targeting CONFIG_UNLOCK (addr 0x0001, mode 0x05) is addressing only and IS allowed
    let read_request_unlock = [0x07, 0x18, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00];
    assert!(transport.write_feature(&read_request_unlock).is_ok());
    assert_eq!(transport.raw_writes.len(), 1);
    assert_eq!(transport.raw_writes[0], read_request_unlock);
}

#[test]
fn write_gate_level2_allows_configuration_writes() {
    let mut transport = MockGateTransport::new(WriteGateLevel::Level2ConfigurationWrite);
    transport.set_identity(panteon_ms89::model::KNOWN_IDENTITIES[0]);

    // Data write (mode 0x03) is allowed under Level 2
    let data_report = [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x00, 0x07];
    assert!(transport.write_feature(&data_report).is_ok());

    // Commit bank (mode 0x09) is allowed under Level 2
    let commit_report = [0x07, 0x18, 0x09, 0x00, 0x18, 0x00, 0x00, 0x07];
    assert!(transport.write_feature(&commit_report).is_ok());

    // Final commit (mode 0x00) is allowed under Level 2
    let final_report = [0x07, 0x18, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00];
    assert!(transport.write_feature(&final_report).is_ok());

    assert_eq!(transport.raw_writes.len(), 3);
}

#[test]
fn write_transaction_plan_dry_run_sequence_set_polling_matches_traces_byte_for_byte() {
    // Session ctrl_f: 500 Hz -> 1000 Hz polling rate configuration
    // Bank 6, Addr 0x0018, Data 0x0700 (1000 Hz)
    let plan = WriteTransactionPlan::new(
        registers::POLLING_RATE.0,
        registers::POLLING_RATE.1,
        PollingRate::Hz1000.to_register_code(),
    );

    // --- PREAMBLE UNLOCK (3 frames) ---
    // P1: Data write (Bank 0, Addr 0x0001, Data 0x0003)
    // Matches ctrl_a_open packet #22: 07 18 03 00 01 00 03 00
    let expected_p1 = [0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00];
    assert_eq!(&plan.preamble_data_frame, &expected_p1);

    // P2: Commit bank for preamble (Bank 0, Addr 0x0001, Data 0x0000)
    // Matches ctrl_a_open packet #24: 07 18 09 00 01 00 00 00
    let expected_p2 = [0x07, 0x18, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00];
    assert_eq!(&plan.preamble_commit_bank_frame, &expected_p2);

    // P3: Final commit for preamble (Bank 0, Addr 0x0001, Data 0x0000)
    // Matches ctrl_a_open packet #26: 07 18 00 00 01 00 00 00
    let expected_p3 = [0x07, 0x18, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00];
    assert_eq!(&plan.preamble_final_commit_frame, &expected_p3);

    // --- MAIN CONFIGURATION (3 frames: write, commit, final) ---
    // Phase 1: Data write (Bank 6, Addr 0x0018, Data 0x0700)
    // Matches ctrl_f line 78: 07 18 03 06 18 00 00 07
    let expected_main_write = [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x00, 0x07];
    assert_eq!(&plan.data_frame, &expected_main_write);

    // Phase 2: Commit bank (Bank 0, Addr 0x0018, Data 0x0700)
    // Matches ctrl_f line 82: 07 18 09 00 18 00 00 07
    let expected_main_commit = [0x07, 0x18, 0x09, 0x00, 0x18, 0x00, 0x00, 0x07];
    assert_eq!(&plan.commit_bank_frame, &expected_main_commit);

    // Phase 3: Final commit (Bank 0, Addr 0x0018, Data 0x0000)
    // Matches ctrl_f line 84: 07 18 00 00 18 00 00 00
    let expected_main_final = [0x07, 0x18, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00];
    assert_eq!(&plan.final_commit_frame, &expected_main_final);

    // --- Phase 4: ACK ---
    // Matches ctrl_f line 86 / ctrl_a_open line 27: 03 00 00 00 00 00 00 00
    let expected_ep82_ack = [0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert_eq!(plan.expected_ack, expected_ep82_ack);

    // Sequence in log_frames: [preamble(3 frames)] + [write, commit, final] + ACK
    let logs = plan.log_frames();
    assert_eq!(logs.len(), 7);
    assert!(logs[0].contains("[07, 18, 03, 00, 01, 00, 03, 00]"));
    assert!(logs[1].contains("[07, 18, 09, 00, 01, 00, 00, 00]"));
    assert!(logs[2].contains("[07, 18, 00, 00, 01, 00, 00, 00]"));
    assert!(logs[3].contains("[07, 18, 03, 06, 18, 00, 00, 07]"));
    assert!(logs[4].contains("[07, 18, 09, 00, 18, 00, 00, 07]"));
    assert!(logs[5].contains("[07, 18, 00, 00, 18, 00, 00, 00]"));
    assert!(logs[6].contains("[03, 00, 00, 00, 00, 00, 00, 00]"));
}

#[test]
fn write_transaction_dry_run_executes_without_wire_write() {
    let mut transport = MockGateTransport::new(WriteGateLevel::Level0ReadOnly);
    let plan = WriteTransactionPlan::new(
        registers::POLLING_RATE.0,
        registers::POLLING_RATE.1,
        PollingRate::Hz1000.to_register_code(),
    );

    // Executing in dry_run mode succeeds under Level 0 without writing any bytes
    let result = transport.execute_transaction(&plan, true);
    assert!(result.is_ok());
    assert!(transport.raw_writes.is_empty());

    // Non-dry-run under Level 0 must return WriteDisabled
    let err = transport.execute_transaction(&plan, false);
    assert_eq!(err, Err(PanteonError::WriteDisabled));
    assert!(transport.raw_writes.is_empty());

    // Non-dry-run under Level 2 writes all 6 frames (preamble 3 + main 3)
    transport.set_write_gate_level(WriteGateLevel::Level2ConfigurationWrite);
    transport.set_identity(panteon_ms89::model::KNOWN_IDENTITIES[0]);
    let ok = transport.execute_transaction(&plan, false);
    assert!(ok.is_ok());
    assert_eq!(transport.raw_writes.len(), 6);
    assert_eq!(transport.raw_writes[0], plan.preamble_data_frame);
    assert_eq!(transport.raw_writes[1], plan.preamble_commit_bank_frame);
    assert_eq!(transport.raw_writes[2], plan.preamble_final_commit_frame);
    assert_eq!(transport.raw_writes[3], plan.data_frame);
    assert_eq!(transport.raw_writes[4], plan.commit_bank_frame);
    assert_eq!(transport.raw_writes[5], plan.final_commit_frame);
}

#[test]
fn write_transaction_rollback_when_main_write_fails() {
    // Fail on write index 4 (the commit bank frame of main sequence, which is W2)
    let mut transport = MockGateTransport::with_failure(WriteGateLevel::Level2ConfigurationWrite, 4);
    transport.set_identity(panteon_ms89::model::KNOWN_IDENTITIES[0]);
    let plan = WriteTransactionPlan::with_initial_data(
        registers::POLLING_RATE.0,
        registers::POLLING_RATE.1,
        PollingRate::Hz1000.to_register_code(),
        Some(PollingRate::Hz500.to_register_code()),
    );

    let res = transport.execute_transaction(&plan, false);
    assert!(res.is_err());
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("frame W2 failed"));

    // Preamble (3 frames) + main data write (1 frame) succeeded before failure at index 4
    // Then rollback issued writes for restoring 500 Hz
    assert!(transport.raw_writes.len() >= 4);
}

#[test]
fn write_transaction_per_frame_diagnostics_phase_names() {
    // Fail on index 0 (P1)
    let mut t0 = MockGateTransport::with_failure(WriteGateLevel::Level2ConfigurationWrite, 0);
    t0.set_identity(panteon_ms89::model::KNOWN_IDENTITIES[0]);
    let plan = WriteTransactionPlan::new(registers::POLLING_RATE.0, registers::POLLING_RATE.1, 0x0700);
    let err0 = t0.execute_transaction(&plan, false).unwrap_err().to_string();
    assert!(err0.contains("frame P1 failed"));

    // Fail on index 1 (P2)
    let mut t1 = MockGateTransport::with_failure(WriteGateLevel::Level2ConfigurationWrite, 1);
    t1.set_identity(panteon_ms89::model::KNOWN_IDENTITIES[0]);
    let err1 = t1.execute_transaction(&plan, false).unwrap_err().to_string();
    assert!(err1.contains("frame P2 failed"));

    // Fail on index 2 (P3)
    let mut t2 = MockGateTransport::with_failure(WriteGateLevel::Level2ConfigurationWrite, 2);
    t2.set_identity(panteon_ms89::model::KNOWN_IDENTITIES[0]);
    let err2 = t2.execute_transaction(&plan, false).unwrap_err().to_string();
    assert!(err2.contains("frame P3 failed"));

    // Fail on index 3 (W1)
    let mut t3 = MockGateTransport::with_failure(WriteGateLevel::Level2ConfigurationWrite, 3);
    t3.set_identity(panteon_ms89::model::KNOWN_IDENTITIES[0]);
    let err3 = t3.execute_transaction(&plan, false).unwrap_err().to_string();
    assert!(err3.contains("frame W1 failed"));

    // Fail on index 4 (W2)
    let mut t4 = MockGateTransport::with_failure(WriteGateLevel::Level2ConfigurationWrite, 4);
    t4.set_identity(panteon_ms89::model::KNOWN_IDENTITIES[0]);
    let err4 = t4.execute_transaction(&plan, false).unwrap_err().to_string();
    assert!(err4.contains("frame W2 failed"));

    // Fail on index 5 (W3)
    let mut t5 = MockGateTransport::with_failure(WriteGateLevel::Level2ConfigurationWrite, 5);
    t5.set_identity(panteon_ms89::model::KNOWN_IDENTITIES[0]);
    let err5 = t5.execute_transaction(&plan, false).unwrap_err().to_string();
    assert!(err5.contains("frame W3 failed"));
}

#[test]
fn experimental_read_register_gate_enforcement_and_mode_05() {
    // 1. Under Level 0: experimental_read_register must fail with WriteDisabled
    let mut t0 = MockGateTransport::new(WriteGateLevel::Level0ReadOnly);
    let res0 = t0.experimental_read_register(6, 0x0018);
    assert_eq!(res0, Err(PanteonError::WriteDisabled));
    assert!(t0.raw_writes.is_empty(), "No hardware writes allowed under Level 0");

    // 2. Under Level 1: experimental_read_register is allowed (addressing write mode 0x05)
    let mut t1 = MockGateTransport::new(WriteGateLevel::Level1AddressingOnly);
    let res1 = t1.experimental_read_register(6, 0x0018);
    assert!(res1.is_ok());

    // Verify sent frame
    assert_eq!(t1.raw_writes.len(), 1);
    let sent_frame = t1.raw_writes[0];
    assert_eq!(sent_frame, [0x07, 0x18, 0x05, 0x06, 0x18, 0x00, 0x00, 0x00]);

    // Critical invariant: mode 0x03 (register data write) must NEVER be transmitted in readback path!
    assert_eq!(sent_frame[2], 0x05);
    assert_ne!(sent_frame[2], 0x03);

    // 3. Verify that mode 0x03 data writes remain strictly blocked under Level 1
    let data_write = [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x00, 0x07];
    assert_eq!(t1.write_feature(&data_write), Err(PanteonError::WriteDisabled));
}

#[test]
fn experimental_read_byte_gate_enforcement_and_sweep_frame() {
    // 1. Under Level 0: experimental_read_byte must fail with WriteDisabled
    let mut t0 = MockGateTransport::new(WriteGateLevel::Level0ReadOnly);
    let res0 = t0.experimental_read_byte(0x0123);
    assert_eq!(res0, Err(PanteonError::WriteDisabled));
    assert!(t0.raw_writes.is_empty(), "No hardware writes allowed under Level 0");

    // 2. Under Level 1: experimental_read_byte is allowed (addressing write mode 0x05)
    let mut t1 = MockGateTransport::new(WriteGateLevel::Level1AddressingOnly);
    let res1 = t1.experimental_read_byte(0x0123);
    assert!(res1.is_ok());

    // Verify sent frame: 07 18 05 00 23 01 00 00
    assert_eq!(t1.raw_writes.len(), 1);
    let sent_frame = t1.raw_writes[0];
    assert_eq!(sent_frame, [0x07, 0x18, 0x05, 0x00, 0x23, 0x01, 0x00, 0x00]);

    // Critical invariant: mode 0x03 (register data write) must NEVER be transmitted in readback path!
    assert_eq!(sent_frame[2], 0x05);
    assert_ne!(sent_frame[2], 0x03);
}

#[test]
fn experimental_read_bank_gate_enforcement_and_negative_checks() {
    // 1. Under Level 0: experimental_read must fail with WriteDisabled
    let mut t0 = MockGateTransport::new(WriteGateLevel::Level0ReadOnly);
    let res0 = t0.experimental_read(6, 0x0018, 4, Duration::ZERO);
    assert_eq!(res0, Err(PanteonError::WriteDisabled));
    assert!(t0.raw_writes.is_empty(), "No hardware writes allowed under Level 0");

    // 2. Under Level 1: experimental_read is allowed
    let mut t1 = MockGateTransport::new(WriteGateLevel::Level1AddressingOnly);
    let res1 = t1.experimental_read(6, 0x0018, 4, Duration::ZERO);
    assert!(res1.is_ok());

    // 4 PREP frames + 1 ARM frame + 4 READ frames + 1 RESET frame = 10 frames
    assert_eq!(t1.raw_writes.len(), 10);

    // Verify first PREP frame (index 0, addr 0x0018, count-1 = 3)
    assert_eq!(t1.raw_writes[0], [0x07, 0x18, 0x09, 0x06, 0x18, 0x00, 0x00, 0x03]);
    // Verify last PREP frame (index 3, addr 0x001b, count-1 = 3)
    assert_eq!(t1.raw_writes[3], [0x07, 0x18, 0x09, 0x06, 0x1b, 0x00, 0x00, 0x03]);
    // Verify ARM frame (mode 0x00, bank 6, addr 0x0018)
    assert_eq!(t1.raw_writes[4], [0x07, 0x18, 0x00, 0x06, 0x18, 0x00, 0x00, 0x00]);
    // Verify first READ frame (mode 0x05, bank 6, addr 0x0018, count-1 = 3)
    assert_eq!(t1.raw_writes[5], [0x07, 0x18, 0x05, 0x06, 0x18, 0x00, 0x00, 0x03]);
    // Verify last READ frame (mode 0x05, bank 6, addr 0x001b, count-1 = 3)
    assert_eq!(t1.raw_writes[8], [0x07, 0x18, 0x05, 0x06, 0x1b, 0x00, 0x00, 0x03]);
    // Verify RESET frame (mode 0x00 all zeros)
    assert_eq!(t1.raw_writes[9], [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // Critical invariant: mode 0x03 must NEVER appear in any sent frame!
    for frame in &t1.raw_writes {
        assert_ne!(frame[2], 0x03, "mode 0x03 must not be sent in readback path");
    }

    // 3. Negative validation checks under Level 1:
    // a) bank > 7
    let err_bank = t1.experimental_read(8, 0x0018, 4, Duration::ZERO);
    assert_eq!(
        err_bank,
        Err(PanteonError::ProtocolDetailed {
            context: "experimental_read",
            detail: "Bank out of range: 8 must be in 0..=7".to_string(),
        })
    );

    // b) count == 0
    let err_count0 = t1.experimental_read(6, 0x0018, 0, Duration::ZERO);
    assert_eq!(
        err_count0,
        Err(PanteonError::ProtocolDetailed {
            context: "experimental_read",
            detail: "read count 0 must be in 1..=256".to_string(),
        })
    );

    // c) count > 256
    let err_count257 = t1.experimental_read(6, 0x0018, 257, Duration::ZERO);
    assert_eq!(
        err_count257,
        Err(PanteonError::ProtocolDetailed {
            context: "experimental_read",
            detail: "read count 257 must be in 1..=256".to_string(),
        })
    );
}

#[test]
fn experimental_read_gate_level_matrix() {
    // Level 0: all read-path write operations must be strictly rejected
    let mut t0 = MockGateTransport::new(WriteGateLevel::Level0ReadOnly);
    assert_eq!(t0.experimental_read(0, 0x0018, 2, Duration::ZERO), Err(PanteonError::WriteDisabled));
    assert_eq!(t0.experimental_read(6, 0x0018, 2, Duration::ZERO), Err(PanteonError::WriteDisabled));
    assert_eq!(t0.experimental_read_chunk(0x0018, 2), Err(PanteonError::WriteDisabled));
    assert_eq!(t0.experimental_read_bank(6, 0x0018, 2), Err(PanteonError::WriteDisabled));
    assert!(t0.raw_writes.is_empty(), "Level 0 must permit zero wire writes");

    // Level 1: read sequences allowed for any bank 0..=7, but data write 0x03 is blocked
    let mut t1 = MockGateTransport::new(WriteGateLevel::Level1AddressingOnly);
    let res_bank0 = t1.experimental_read(0, 0x0100, 2, Duration::ZERO);
    assert!(res_bank0.is_ok());
    // 2 PREP + 1 ARM + 2 READ + 1 RESET = 6 frames
    assert_eq!(t1.raw_writes.len(), 6);
    for frame in &t1.raw_writes {
        assert_ne!(frame[2], 0x03, "Level 1 must never emit mode 0x03");
    }

    // Direct write with mode 0x03 must be blocked under Level 1
    let data_frame = [0x07, 0x18, 0x03, 0x00, 0x00, 0x01, 0x01, 0x00];
    assert_eq!(t1.write_feature(&data_frame), Err(PanteonError::WriteDisabled));

    // Level 2: read sequences and configuration writes both allowed
    let mut t2 = MockGateTransport::new(WriteGateLevel::Level2ConfigurationWrite);
    t2.set_identity(panteon_ms89::model::KNOWN_IDENTITIES[0]);
    let res_l2_read = t2.experimental_read(5, 0x0020, 2, Duration::ZERO);
    assert!(res_l2_read.is_ok());
    assert_eq!(t2.raw_writes.len(), 6);

    // Direct write with mode 0x03 is allowed under Level 2
    assert!(t2.write_feature(&data_frame).is_ok());
    assert_eq!(t2.raw_writes.len(), 7);
}

#[test]
fn experimental_read_descriptor_chunk_gate_enforcement() {
    // 1. Under Level 0: experimental_read(0, 0, 32) must fail with WriteDisabled
    let mut t0 = MockGateTransport::new(WriteGateLevel::Level0ReadOnly);
    let res0 = t0.experimental_read(0, 0, 32, Duration::ZERO);
    assert_eq!(res0, Err(PanteonError::WriteDisabled));
    assert!(t0.raw_writes.is_empty(), "No hardware writes allowed under Level 0");

    // 2. Under Level 1: experimental_read(0, 0, 32) is allowed
    let mut t1 = MockGateTransport::new(WriteGateLevel::Level1AddressingOnly);
    let res1 = t1.experimental_read(0, 0, 32, Duration::ZERO);
    assert!(res1.is_ok());
    // 32 PREP frames + 1 ARM frame + 32 READ frames + 1 RESET frame = 66 frames
    assert_eq!(t1.raw_writes.len(), 66);
    // Verify first PREP frame (index 0, addr 0x0000, count-1 = 31)
    assert_eq!(t1.raw_writes[0], [0x07, 0x18, 0x09, 0x00, 0x00, 0x00, 0x00, 0x1f]);
    // Verify second PREP frame (index 1, addr 0x0000, count-1 = 31)
    assert_eq!(t1.raw_writes[1], [0x07, 0x18, 0x09, 0x01, 0x00, 0x00, 0x00, 0x1f]);
    // Verify ARM frame (mode 0x00, bank 0, addr 0x0000)
    assert_eq!(t1.raw_writes[32], [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    // Verify first READ frame (mode 0x05, index 0, addr 0x0000, count-1 = 31)
    assert_eq!(t1.raw_writes[33], [0x07, 0x18, 0x05, 0x00, 0x00, 0x00, 0x00, 0x1f]);
    // Verify RESET frame (mode 0x00 all zeros)
    assert_eq!(t1.raw_writes[65], [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn write_gate_level2_unverified_identity_rejected() {
    let unverified_identity = DeviceIdentity {
        pid: 0x1e01,
        protocol_verified: false,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let mut transport = MockGateTransport::with_identity(
        WriteGateLevel::Level2ConfigurationWrite,
        unverified_identity,
    );
    let plan = WriteTransactionPlan::new(registers::POLLING_RATE.0, registers::POLLING_RATE.1, 0x0700);

    // 1. execute_transaction must return Err(PanteonError::ProtocolDetailed)
    let err = transport.execute_transaction(&plan, false).unwrap_err();
    match err {
        PanteonError::ProtocolDetailed { context, detail } => {
            assert_eq!(context, "write_transaction");
            assert!(detail.contains("protocol unverified for PID 0x1e01"));
        }
        _ => panic!("Expected ProtocolDetailed error, got {:?}", err),
    }

    // 2. write_feature must also be rejected
    let write_err = transport.write_feature(&plan.data_frame).unwrap_err();
    match write_err {
        PanteonError::ProtocolDetailed { context, detail } => {
            assert_eq!(context, "write_feature");
            assert!(detail.contains("protocol unverified for PID 0x1e01"));
        }
        _ => panic!("Expected ProtocolDetailed error, got {:?}", write_err),
    }

    // 3. Dry run should still succeed (no hardware write executed)
    assert!(transport.execute_transaction(&plan, true).is_ok());
}

#[test]
fn write_gate_level2_mock_default_identity_fail_safe_rejected() {
    // A mock without explicit set_identity defaults to DeviceIdentity::unknown() (pid: 0, protocol_verified: false)
    let mut transport = MockGateTransport::new(WriteGateLevel::Level2ConfigurationWrite);
    let plan = WriteTransactionPlan::new(registers::POLLING_RATE.0, registers::POLLING_RATE.1, 0x0700);

    // 1. execute_transaction fails with ProtocolDetailed unverified
    let err = transport.execute_transaction(&plan, false).unwrap_err();
    match err {
        PanteonError::ProtocolDetailed { context, detail } => {
            assert_eq!(context, "write_transaction");
            assert!(detail.contains("protocol unverified for PID 0x0000"));
        }
        _ => panic!("Expected ProtocolDetailed error, got {:?}", err),
    }

    // 2. write_feature fails with ProtocolDetailed unverified
    let write_err = transport.write_feature(&plan.data_frame).unwrap_err();
    match write_err {
        PanteonError::ProtocolDetailed { context, detail } => {
            assert_eq!(context, "write_feature");
            assert!(detail.contains("protocol unverified for PID 0x0000"));
        }
        _ => panic!("Expected ProtocolDetailed error, got {:?}", write_err),
    }
}

#[test]
fn write_gate_level2_trait_default_identity_fail_safe() {
    struct BareMock {
        gate_level: WriteGateLevel,
    }
    impl Transport for BareMock {
        fn read_feature(&mut self, _report: &mut [u8; FEATURE_REPORT_LEN]) -> Result<(), PanteonError> {
            Ok(())
        }
        fn write_gate_level(&self) -> WriteGateLevel {
            self.gate_level
        }
        fn set_write_gate_level(&mut self, level: WriteGateLevel) {
            self.gate_level = level;
        }
        fn raw_write_feature(&mut self, _report: &[u8; FEATURE_REPORT_LEN]) -> Result<(), PanteonError> {
            Ok(())
        }
    }

    let mut bare = BareMock {
        gate_level: WriteGateLevel::Level2ConfigurationWrite,
    };
    assert_eq!(bare.device_identity(), DeviceIdentity::unknown());
    let plan = WriteTransactionPlan::new(registers::POLLING_RATE.0, registers::POLLING_RATE.1, 0x0700);
    let err = bare.execute_transaction(&plan, false).unwrap_err();
    match err {
        PanteonError::ProtocolDetailed { context, detail } => {
            assert_eq!(context, "write_transaction");
            assert!(detail.contains("protocol unverified for PID 0x0000"));
        }
        _ => panic!("Expected ProtocolDetailed error, got {:?}", err),
    }
}

#[test]
fn write_gate_level2_verified_identity_accepted() {
    let verified_identity = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: true,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let mut transport = MockGateTransport::with_identity(
        WriteGateLevel::Level2ConfigurationWrite,
        verified_identity,
    );
    let plan = WriteTransactionPlan::new(registers::POLLING_RATE.0, registers::POLLING_RATE.1, 0x0700);

    assert!(transport.execute_transaction(&plan, false).is_ok());
}

#[test]
fn execute_transaction_unchecked_is_crate_private() {
    let lib_source = include_str!("../src/device.rs");
    assert!(!lib_source.contains("pub fn execute_transaction_unchecked"));
    assert!(!lib_source.contains("pub trait Transport {") && lib_source.contains("pub trait Transport"), "trait exists");
    let trait_block = lib_source.split("pub trait Transport").nth(1).unwrap();
    let trait_end = trait_block.split('}').next().unwrap();
    assert!(!trait_end.contains("execute_transaction_unchecked"),
        "Transport trait must NOT expose execute_transaction_unchecked");
}

#[test]
fn execute_with_commit_dry_run_does_not_append_tail_or_write() {
    let verified_identity = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: true,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let mut transport = MockGateTransport::with_identity(
        WriteGateLevel::Level2ConfigurationWrite,
        verified_identity,
    );
    let plan = WriteTransactionPlan::new(registers::ACTIVE_PROFILE.0, registers::ACTIVE_PROFILE.1, 0x0700);

    panteon_ms89::service::execute_with_commit(&mut transport, &plan, true, true)
        .expect("dry_run should succeed");
    assert_eq!(transport.raw_writes.len(), 0, "dry_run must not write any frames including tail");
}

#[test]
fn execute_with_commit_gate_enforcement_and_flash_flush() {
    let unverified = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: false,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let verified = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: true,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let plan = WriteTransactionPlan::new(registers::ACTIVE_PROFILE.0, registers::ACTIVE_PROFILE.1, 0x0700);

    // 1. Level 0 blocks execute_with_commit
    let mut t0 = MockGateTransport::with_identity(WriteGateLevel::Level0ReadOnly, verified);
    assert_eq!(
        panteon_ms89::service::execute_with_commit(&mut t0, &plan, false, false),
        Err(PanteonError::WriteDisabled)
    );

    // 2. Level 1 blocks data writes
    let mut t1 = MockGateTransport::with_identity(WriteGateLevel::Level1AddressingOnly, verified);
    assert_eq!(
        panteon_ms89::service::execute_with_commit(&mut t1, &plan, false, false),
        Err(PanteonError::WriteDisabled)
    );

    // 3. Level 2 unverified identity rejected
    let mut t2_unverified = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, unverified);
    assert!(panteon_ms89::service::execute_with_commit(&mut t2_unverified, &plan, false, false).is_err());

    // 4. Level 2 verified identity: commit_flash = false -> 6 transaction + 4 tail = 10 frames
    let mut t2_no_flush = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, verified);
    panteon_ms89::service::execute_with_commit(&mut t2_no_flush, &plan, false, false)
        .expect("write with commit (no flash flush) must succeed");
    assert_eq!(t2_no_flush.raw_writes.len(), 10);
    // Ensure 0x10 mode is NOT in the sent frames
    assert!(!t2_no_flush.raw_writes.iter().any(|f| f[1] == 0x18 && f[2] == 0x10));
    // Check tail frames
    assert_eq!(t2_no_flush.raw_writes[6], [0x07, 0x18, 0x03, 0x00, 0x7f, 0x00, 0x00, 0x00]);
    assert_eq!(t2_no_flush.raw_writes[7], [0x07, 0x18, 0x09, 0x00, 0x7f, 0x00, 0x00, 0x00]);
    assert_eq!(t2_no_flush.raw_writes[8], [0x07, 0x18, 0x00, 0x00, 0x7f, 0x00, 0x00, 0x00]);
    assert_eq!(t2_no_flush.raw_writes[9], [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // 5. Level 2 verified identity: commit_flash = true -> 6 transaction + 5 tail = 11 frames
    let mut t2_flush = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, verified);
    panteon_ms89::service::execute_with_commit(&mut t2_flush, &plan, false, true)
        .expect("write with commit (with flash flush) must succeed");
    assert_eq!(t2_flush.raw_writes.len(), 11);
    // Frame 9 (index 9, i.e. 10th frame) must be mode 0x10
    assert_eq!(t2_flush.raw_writes[9], [0x07, 0x18, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]);
    // Frame 10 must be idle 0x00
    assert_eq!(t2_flush.raw_writes[10], [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn test_soft_reset_eacces_or_io_error() {
    let res = panteon_ms89::service::soft_reset((999, 999));
    assert!(res.is_err());
    let err_str = res.unwrap_err().to_string();
    assert!(err_str.contains("Failed to open") || err_str.contains("EACCES") || err_str.contains("нужен udev-доступ"));
}

#[test]
fn execute_fire_delay_gate_and_commit_tail() {
    let verified = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: true,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let unverified = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: false,
        accepted_signatures: &[0x0976, 0x0978],
    };

    let mut ledger = ShadowLedger::default();
    ledger.append(ShadowEntry::new(0, 0x0018, 0x0718, "1000"));

    // 1. Dry run writes 0 frames
    let mut t_dry = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, verified);
    panteon_ms89::service::execute_fire_delay_with_ledger(&mut t_dry, 50, &ledger, true, true, 0)
        .expect("dry run fire delay should succeed");
    assert_eq!(t_dry.raw_writes.len(), 0);

    // 2. Level 0 blocks write
    let mut t0 = MockGateTransport::with_identity(WriteGateLevel::Level0ReadOnly, verified);
    assert_eq!(
        panteon_ms89::service::execute_fire_delay_with_ledger(&mut t0, 50, &ledger, false, false, 0),
        Err(PanteonError::WriteDisabled)
    );

    // 3. Level 1 blocks write
    let mut t1 = MockGateTransport::with_identity(WriteGateLevel::Level1AddressingOnly, verified);
    assert_eq!(
        panteon_ms89::service::execute_fire_delay_with_ledger(&mut t1, 50, &ledger, false, false, 0),
        Err(PanteonError::WriteDisabled)
    );

    // 4. Level 2 unverified identity rejected
    let mut t2_unverified = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, unverified);
    assert!(panteon_ms89::service::execute_fire_delay_with_ledger(&mut t2_unverified, 50, &ledger, false, false, 0).is_err());

    // 5. Level 2 verified identity: commit_flash = false -> 1 block = 3 preamble + 10 block + 4 tail = 17 frames
    let mut t2_no_flush = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, verified);
    panteon_ms89::service::execute_fire_delay_with_ledger(&mut t2_no_flush, 50, &ledger, false, false, 0)
        .expect("execute fire delay (no flash flush) must succeed");
    assert_eq!(t2_no_flush.raw_writes.len(), 17);
    // Ensure no 0x10 mode frame in writes
    assert!(!t2_no_flush.raw_writes.iter().any(|f| f[1] == 0x18 && f[2] == 0x10));

    // 6. Level 2 verified identity: commit_flash = true -> 1 block = 3 preamble + 10 block + 5 tail = 18 frames
    let mut t2_flush = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, verified);
    panteon_ms89::service::execute_fire_delay_with_ledger(&mut t2_flush, 300, &ledger, false, true, 0)
        .expect("execute fire delay (with flash flush) must succeed");
    assert_eq!(t2_flush.raw_writes.len(), 18);
    // Frame 16 (17th frame) must be mode 0x10
    assert_eq!(t2_flush.raw_writes[16], [0x07, 0x18, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]);
    // Frame 17 (18th frame) must be idle 0x00
    assert_eq!(t2_flush.raw_writes[17], [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn test_execute_plans_with_commit_rejects_multi_bank_block() {
    let verified = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: true,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let mut transport = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, verified);

    let multi_addrs = [0x0010, 0x0018, 0x0020, 0x0028, 0x0030, 0x0038];
    for &addr in &multi_addrs {
        let plan = WriteTransactionPlan::new(0, addr, 0x0700);
        let err = panteon_ms89::service::execute_plans_with_commit(&mut transport, &[plan.clone()], false, false, 0)
            .unwrap_err();
        assert_eq!(
            err,
            PanteonError::Protocol("multi-bank block requires block session — see ADR-003 §7")
        );

        // Dry run must also reject
        let dry_err = panteon_ms89::service::execute_plans_with_commit(&mut transport, &[plan], true, false, 0)
            .unwrap_err();
        assert_eq!(
            dry_err,
            PanteonError::Protocol("multi-bank block requires block session — see ADR-003 §7")
        );
    }

    // Single-bank register (e.g. 0x0070) must be accepted
    let single_plan = WriteTransactionPlan::new(registers::ACTIVE_PROFILE.0, registers::ACTIVE_PROFILE.1, 0x0700);
    assert!(panteon_ms89::service::execute_plans_with_commit(&mut transport, &[single_plan], true, false, 0).is_ok());
}

#[test]
fn test_resolve_block18_from_ledger() {
    let mut ledger = ShadowLedger::new("/dev/hidraw1");
    ledger.append(ShadowEntry::new(0, 0x0018, 0x0719, "1000"));
    ledger.append(ShadowEntry::new(1, 0x0018, 0x073f, "1001"));
    ledger.append(ShadowEntry::new(2, 0x0018, 0x0764, "1002"));
    ledger.append(ShadowEntry::new(3, 0x0018, 0x0700, "1003"));
    ledger.append(ShadowEntry::new(4, 0x0018, 0x0782, "1004"));
    ledger.append(ShadowEntry::new(5, 0x0018, 0x07fe, "1005"));
    ledger.append(ShadowEntry::new(6, 0x0018, 0x0707, "1006")); // 125 Hz in ledger
    ledger.append(ShadowEntry::new(7, 0x0018, 0x0700, "1007"));

    let block = panteon_ms89::service::resolve_block18(&ledger, &[(6, 0x0701)]).unwrap();
    assert_eq!(block.addr, 0x0018);
    assert_eq!(block.banks[0], 0x0719);
    assert_eq!(block.banks[1], 0x073f);
    assert_eq!(block.banks[2], 0x0764);
    assert_eq!(block.banks[3], 0x0700);
    assert_eq!(block.banks[4], 0x0782);
    assert_eq!(block.banks[5], 0x07fe);
    assert_eq!(block.banks[6], 0x0701); // Overridden to 500 Hz
    assert_eq!(block.banks[7], 0x0700);
}

#[test]
fn test_resolve_block18_fallback_to_factory_reset() {
    let mut ledger = ShadowLedger::default();
    ledger.append(ShadowEntry::new(0, 0x0018, 0x0718, "1000"));
    let block = panteon_ms89::service::resolve_block18(&ledger, &[]).unwrap();

    assert_eq!(block.addr, 0x0018);
    assert_eq!(block.banks[0], 0x0718);
    assert_eq!(block.banks[1], 0x073f);
    assert_eq!(block.banks[2], 0x0732);
    assert_eq!(block.banks[3], 0x0700);
    assert_eq!(block.banks[4], 0x07f3);
    assert_eq!(block.banks[5], 0x07ff);
    assert_eq!(block.banks[6], 0x0701);
    assert_eq!(block.banks[7], 0x0700);
}

#[test]
fn test_set_polling_uses_block_session() {
    let verified = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: true,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let mut transport = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, verified);

    let mut ledger = ShadowLedger::default();
    ledger.append(ShadowEntry::new(0, 0x0018, 0x0718, "1000"));
    let block18 = panteon_ms89::service::resolve_block18(&ledger, &[(6, 0x0701)]).unwrap();
    assert_eq!(block18.addr, 0x0018);
    assert_eq!(block18.banks[6], 0x0701);

    // Build ordered sequence of wire frames
    let session_frames = panteon_ms89::service::plan_block_session_frames(&[block18.clone()], None, false);
    assert_eq!(session_frames.len(), 17);

    // Verify byte-exact preamble frames matching golden structure
    assert_eq!(session_frames[0], [0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x23, 0x00]);
    assert_eq!(session_frames[1], [0x07, 0x18, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00]);
    assert_eq!(session_frames[2], [0x07, 0x18, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]);

    // Verify 8 data banks @ 0x0018
    let expected_data = [
        [0x07, 0x18, 0x03, 0x00, 0x18, 0x00, 0x18, 0x07], // Bank 0: 0x0718
        [0x07, 0x18, 0x03, 0x01, 0x18, 0x00, 0x3f, 0x07], // Bank 1: 0x073f
        [0x07, 0x18, 0x03, 0x02, 0x18, 0x00, 0x32, 0x07], // Bank 2: 0x0732
        [0x07, 0x18, 0x03, 0x03, 0x18, 0x00, 0x00, 0x07], // Bank 3: 0x0700
        [0x07, 0x18, 0x03, 0x04, 0x18, 0x00, 0xf3, 0x07], // Bank 4: 0x07f3
        [0x07, 0x18, 0x03, 0x05, 0x18, 0x00, 0xff, 0x07], // Bank 5: 0x07ff
        [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x01, 0x07], // Bank 6: 0x0701 (500 Hz)
        [0x07, 0x18, 0x03, 0x07, 0x18, 0x00, 0x00, 0x07], // Bank 7: 0x0700
    ];
    for b in 0..8usize {
        assert_eq!(session_frames[3 + b], expected_data[b]);
    }

    // Verify commit frames for block 0x0018
    assert_eq!(session_frames[11], [0x07, 0x18, 0x09, 0x00, 0x18, 0x00, 0x00, 0x07]);
    assert_eq!(session_frames[12], [0x07, 0x18, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00]);

    // Verify soft-apply tail
    assert_eq!(session_frames[13], [0x07, 0x18, 0x03, 0x00, 0x7f, 0x00, 0x00, 0x00]);
    assert_eq!(session_frames[14], [0x07, 0x18, 0x09, 0x00, 0x7f, 0x00, 0x00, 0x00]);
    assert_eq!(session_frames[15], [0x07, 0x18, 0x00, 0x00, 0x7f, 0x00, 0x00, 0x00]);
    assert_eq!(session_frames[16], [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // Execute via execute_block_session and verify transport output
    panteon_ms89::service::execute_block_session(&mut transport, &[block18], false, false, 0)
        .expect("execute_block_session for polling must succeed");
    assert_eq!(transport.raw_writes.len(), 17);
    assert_eq!(&transport.raw_writes[..17], &session_frames[..17]);
}

#[test]
fn test_dry_run_set_polling_250_hz_plan() {
    let mut ledger = ShadowLedger::default();
    ledger.append(ShadowEntry::new(0, 0x0018, 0x0718, 1000u64));

    let rate = PollingRate::Hz250;
    let code = rate.to_register_code();
    assert_eq!(code, 0x0703);

    let block18 = panteon_ms89::service::resolve_block18(&ledger, &[(registers::POLLING_RATE.0, code)]).unwrap();
    assert_eq!(block18.addr, 0x0018);
    assert_eq!(block18.banks[6], 0x0703);

    let session_frames = panteon_ms89::service::plan_block_session_frames(&[block18], None, false);
    assert_eq!(session_frames.len(), 17, "Planned block session for polling must contain exactly 17 frames");

    // Frame index 9 is Bank 6 data write (preamble: 3 frames (0..=2), data banks: 3..=10 (bank 6 is 3 + 6 = 9))
    let bank6_frame = session_frames[9];
    assert_eq!(
        bank6_frame,
        [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x03, 0x07],
        "Bank 6 frame must contain 0x0703 (250 Hz) matching OEM trace ctrl_u_polling_500-250.pcap"
    );

    assert!(
        session_frames.iter().any(|f| *f == [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x03, 0x07]),
        "17-frame plan must contain bank6=0x0703 write frame"
    );
}

#[test]
fn marker_check_ignores_volatile_handshake_byte() {
    let temp_dir = std::env::temp_dir().join(format!("test_marker_mock_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let marker_file = temp_dir.join("verified.toml");

    let mut store = panteon_ms89::store::VerifiedStore::default();
    store.insert_or_update(0x1e01, vec![0x0976], 1762970000);
    panteon_ms89::store::save_verified_store_atomic(&marker_file, &store).unwrap();

    // Mock transport with unverified PID 0x1e01
    let unverified = DeviceIdentity {
        pid: 0x1e01,
        protocol_verified: false,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let mut transport = MockGateTransport::with_identity(
        WriteGateLevel::Level2ConfigurationWrite,
        unverified,
    );

    // ioctl returns report with buf[1] = 0xff and signature 0x0976 (matches marker)
    let report_ff = [0x07, 0xff, 0x76, 0x09, 0x1e, 0x00, 0x23, 0x7b];
    let matched = transport.check_persisted_marker(&report_ff, Some(&marker_file));
    assert!(matched);
    assert!(transport.device_identity().protocol_verified);

    // Signature not from marker (0x0978) -> false
    let unverified_mismatch = DeviceIdentity {
        pid: 0x1e01,
        protocol_verified: false,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let mut transport_mismatch = MockGateTransport::with_identity(
        WriteGateLevel::Level2ConfigurationWrite,
        unverified_mismatch,
    );
    let report_mismatch = [0x07, 0xff, 0x78, 0x09, 0x1e, 0x00, 0x23, 0x7b];
    let matched_mismatch = transport_mismatch.check_persisted_marker(&report_mismatch, Some(&marker_file));
    assert!(!matched_mismatch);
    assert!(!transport_mismatch.device_identity().protocol_verified);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_block_session_golden_sequence() {
    let verified = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: true,
        accepted_signatures: &[0x0976, 0x0978],
    };

    // Synthetic session for 200/800 R/B scenario
    let slots = [200, 800, 200, 800, 200, 800];
    let dpi_plans = panteon_ms89::service::plan_dpi_slots(&slots).unwrap();

    let main_rb: [(u8, u8, u8); 6] = [
        (255, 0, 0),
        (0, 0, 255),
        (255, 0, 0),
        (0, 0, 255),
        (255, 0, 0),
        (0, 0, 255),
    ];
    let second_rb = panteon_ms89::codec::DEFAULT_SECOND_COLORS;
    let mut ledger = ShadowLedger::default();
    ledger.append(ShadowEntry::new(7, 0x0038, 0x0700, "1000"));
    let color_plans = panteon_ms89::service::plan_color_session(&main_rb, &second_rb, &ledger, None)
        .expect("plan_color_session should succeed");

    let mut plans = Vec::new();
    plans.extend_from_slice(&dpi_plans);
    plans.extend(color_plans);

    let blocks = panteon_ms89::service::plans_to_block_writes(&plans);
    assert_eq!(blocks.len(), 5);

    let mut transport = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, verified);
    panteon_ms89::service::execute_block_session(&mut transport, &blocks, false, true, 0)
        .expect("execute_block_session must succeed");

    assert_eq!(transport.raw_writes.len(), 58);

    // Frame 0..2: Preamble (3 frames: P1 Data Write, P2 Commit Bank, P3 Final Commit)
    assert_eq!(transport.raw_writes[0], [0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x23, 0x00]);
    assert_eq!(transport.raw_writes[1], [0x07, 0x18, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00]);
    assert_eq!(transport.raw_writes[2], [0x07, 0x18, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]);

    // Block 0x0010 (DPI slots: 8 data + 1 bank commit + 1 final commit)
    let expected_0x10 = [0x00u8, 0x00, 0x00, 0x00, 0x08, 0x14, 0x14, 0x14];
    for b in 0..8usize {
        assert_eq!(transport.raw_writes[3 + b], [0x07, 0x18, 0x03, b as u8, 0x10, 0x00, expected_0x10[b], 0x07]);
    }
    assert_eq!(transport.raw_writes[11], [0x07, 0x18, 0x09, 0x00, 0x10, 0x00, 0x00, 0x07]);
    assert_eq!(transport.raw_writes[12], [0x07, 0x18, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00]);

    // Block 0x0020
    let expected_0x20 = [0x00u8, 0x1f, 0xff, 0xff, 0x8f, 0x00, 0x00, 0xff];
    for b in 0..8usize {
        assert_eq!(transport.raw_writes[13 + b], [0x07, 0x18, 0x03, b as u8, 0x20, 0x00, expected_0x20[b], 0x07]);
    }
    assert_eq!(transport.raw_writes[21], [0x07, 0x18, 0x09, 0x00, 0x20, 0x00, 0x00, 0x07]);
    assert_eq!(transport.raw_writes[22], [0x07, 0x18, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00]);

    // Block 0x0028
    let expected_0x28 = [0x8f, 0xff, 0x0f, 0xb0, 0x00, 0xff, 0xff, 0xff];
    for b in 0..8usize {
        assert_eq!(transport.raw_writes[23 + b], [0x07, 0x18, 0x03, b as u8, 0x28, 0x00, expected_0x28[b], 0x07]);
    }
    assert_eq!(transport.raw_writes[31], [0x07, 0x18, 0x09, 0x00, 0x28, 0x00, 0x00, 0x07]);
    assert_eq!(transport.raw_writes[32], [0x07, 0x18, 0x00, 0x00, 0x28, 0x00, 0x00, 0x00]);

    // Block 0x0030
    let expected_0x30 = [0x0f, 0x00, 0xff, 0x7f, 0x00, 0x00, 0x00, 0x00];
    for b in 0..8usize {
        assert_eq!(transport.raw_writes[33 + b], [0x07, 0x18, 0x03, b as u8, 0x30, 0x00, expected_0x30[b], 0x07]);
    }
    assert_eq!(transport.raw_writes[41], [0x07, 0x18, 0x09, 0x00, 0x30, 0x00, 0x00, 0x07]);
    assert_eq!(transport.raw_writes[42], [0x07, 0x18, 0x00, 0x00, 0x30, 0x00, 0x00, 0x00]);

    // Block 0x0038
    let expected_0x38 = [0x00, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x8f, 0x00];
    for b in 0..8usize {
        assert_eq!(transport.raw_writes[43 + b], [0x07, 0x18, 0x03, b as u8, 0x38, 0x00, expected_0x38[b], 0x07]);
    }
    assert_eq!(transport.raw_writes[51], [0x07, 0x18, 0x09, 0x00, 0x38, 0x00, 0x00, 0x07]);
    assert_eq!(transport.raw_writes[52], [0x07, 0x18, 0x00, 0x00, 0x38, 0x00, 0x00, 0x00]);

    // Commit tail: 0x007f (3 frames) + Flash Flush (0x10) + idle (0x00)
    assert_eq!(transport.raw_writes[53], [0x07, 0x18, 0x03, 0x00, 0x7f, 0x00, 0x00, 0x00]);
    assert_eq!(transport.raw_writes[54], [0x07, 0x18, 0x09, 0x00, 0x7f, 0x00, 0x00, 0x00]);
    assert_eq!(transport.raw_writes[55], [0x07, 0x18, 0x00, 0x00, 0x7f, 0x00, 0x00, 0x00]);
    assert_eq!(transport.raw_writes[56], [0x07, 0x18, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(transport.raw_writes[57], [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn test_block_session_gate() {
    let verified = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: true,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let unverified = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: false,
        accepted_signatures: &[0x0976, 0x0978],
    };

    let block = panteon_ms89::device::BlockWrite::new(0x0010, [0x0700; 8]);
    let blocks = [block];

    // 1. Dry run writes 0 frames
    let mut t_dry = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, verified);
    panteon_ms89::service::execute_block_session(&mut t_dry, &blocks, true, true, 0)
        .expect("dry run should succeed");
    assert_eq!(t_dry.raw_writes.len(), 0);

    // 2. Level 0 blocks write
    let mut t0 = MockGateTransport::with_identity(WriteGateLevel::Level0ReadOnly, verified);
    assert_eq!(
        panteon_ms89::service::execute_block_session(&mut t0, &blocks, false, false, 0),
        Err(PanteonError::WriteDisabled)
    );

    // 3. Level 1 blocks write
    let mut t1 = MockGateTransport::with_identity(WriteGateLevel::Level1AddressingOnly, verified);
    assert_eq!(
        panteon_ms89::service::execute_block_session(&mut t1, &blocks, false, false, 0),
        Err(PanteonError::WriteDisabled)
    );

    // 4. Level 2 unverified identity rejected
    let mut t2_unverified = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, unverified);
    assert!(panteon_ms89::service::execute_block_session(&mut t2_unverified, &blocks, false, false, 0).is_err());
}

#[test]
fn ledger_slot4_led_write_keeps_slot() {
    let mut ledger = ShadowLedger::new("/dev/hidraw1");
    // Slot 4 in ledger: bank 0 = 0x071b (0x18 + 4 - 1 = 0x1b)
    ledger.append(ShadowEntry::new(0, 0x0018, 0x071b, "1000"));

    // LED write transaction (effect 0x8, speed 0x2 -> data 0x0782)
    let plan = panteon_ms89::service::plan_led_effect_speed(0x08, 0x02).unwrap();
    let block = panteon_ms89::service::resolve_block18(&ledger, &[(plan.bank, plan.data)])
        .expect("resolve_block18 for LED write should succeed");

    assert_eq!(block.addr, 0x0018);
    // Bank 0 must preserve slot 4 (0x071b), NOT reset to slot 1 (0x0718)!
    assert_eq!(block.banks[0], 0x071b);
    assert_eq!(block.banks[4], 0x0782);

    // Verify wire frames retain slot 4 (frame 3 is bank 0)
    let session_frames = panteon_ms89::service::plan_block_session_frames(&[block], None, false);
    assert_eq!(session_frames[3], [0x07, 0x18, 0x03, 0x00, 0x18, 0x00, 0x1b, 0x07]);
}

#[test]
fn unknown_slot_rejected() {
    let empty_ledger = ShadowLedger::default();

    // 1. Empty ledger, no override -> PanteonError::Protocol
    let err = panteon_ms89::service::resolve_block18(&empty_ledger, &[]).unwrap_err();
    assert_eq!(
        err,
        PanteonError::Protocol("active DPI slot unknown, supply --active-slot / set slot in GUI")
    );

    // 2. LED write with empty ledger and no override -> PanteonError, NOT 0x0718
    let led_plan = panteon_ms89::service::plan_led_effect_speed(0x08, 0x02).unwrap();
    let err2 = panteon_ms89::service::resolve_block18(&empty_ledger, &[(led_plan.bank, led_plan.data)]).unwrap_err();
    assert_eq!(
        err2,
        PanteonError::Protocol("active DPI slot unknown, supply --active-slot / set slot in GUI")
    );

    // 3. Polling write with empty ledger and no override -> PanteonError
    let err3 = panteon_ms89::service::resolve_block18(&empty_ledger, &[(6, 0x0701)]).unwrap_err();
    assert_eq!(
        err3,
        PanteonError::Protocol("active DPI slot unknown, supply --active-slot / set slot in GUI")
    );

    // 4. Empty reset snapshot + empty ledger also gives error, NOT 0x0000
    let err4 = panteon_ms89::service::resolve_block18_internal(&empty_ledger, &[], &[None; 8]).unwrap_err();
    assert_eq!(
        err4,
        PanteonError::Protocol("active DPI slot unknown, supply --active-slot / set slot in GUI")
    );
}

#[test]
fn test_cli_write_and_gui_actions_dry_run_show_bank0_from_ledger() {
    let mut ledger = ShadowLedger::new("/dev/hidraw1");
    ledger.append(ShadowEntry::new(0, 0x0018, 0x071b, "1000")); // Slot 4

    // CLI dry-run resolution for polling write preserves bank 0 = 0x071b from ledger
    let block18 = panteon_ms89::service::resolve_block18(&ledger, &[(registers::POLLING_RATE.0, 0x0701)]).unwrap();
    assert_eq!(block18.banks[0], 0x071b);
    let frames = panteon_ms89::service::plan_block_session_frames(&[block18], None, false);
    assert_eq!(frames[3], [0x07, 0x18, 0x03, 0x00, 0x18, 0x00, 0x1b, 0x07]);

    #[cfg(feature = "gui")]
    {
        let mut app = panteon_ms89::gui::app::PanteonApp::default();
        app.ledger.append(ShadowEntry::new(0, 0x0018, 0x071b, "1000"));
        app.plan_polling();
        assert!(
            app.status_message.contains("Bank 0: 0x071b"),
            "GUI plan_polling dry-run must show bank0 from ledger, got: {}",
            app.status_message
        );

        app.plan_led();
        assert!(
            app.status_message.contains("Bank 0: 0x071b"),
            "GUI plan_led dry-run must show bank0 from ledger, got: {}",
            app.status_message
        );

        app.plan_fire_delay_action();
        assert!(
            app.status_message.contains("Bank 0: 0x071b"),
            "GUI plan_fire_delay_action dry-run must show bank0 from ledger, got: {}",
            app.status_message
        );
    }
}

#[test]
fn test_execute_active_dpi_slot_records_to_ledger() {
    let verified = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: true,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let mut transport = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, verified);
    let mut ledger = ShadowLedger::default();

    // Execute active slot 4
    panteon_ms89::service::execute_active_dpi_slot(&mut transport, 4, &mut ledger, false, false, 0)
        .expect("execute_active_dpi_slot should succeed");

    // Check that ledger recorded the write
    assert_eq!(ledger.entries.len(), 1);
    assert_eq!(ledger.entries[0].bank, 0);
    assert_eq!(ledger.entries[0].addr, 0x0018);
    assert_eq!(ledger.entries[0].data, 0x071b);

    // Verify wire frame for Bank 0 @ 0x0018 is slot 4
    assert_eq!(transport.raw_writes.len(), 17);
    assert_eq!(transport.raw_writes[3], [0x07, 0x18, 0x03, 0x00, 0x18, 0x00, 0x1b, 0x07]);
}

#[test]
fn color_write_with_ledger_bank7() {
    let main_rb: [(u8, u8, u8); 6] = [
        (255, 0, 0), (0, 0, 255), (255, 0, 0),
        (0, 0, 255), (255, 0, 0), (0, 0, 255),
    ];
    let second_rb = panteon_ms89::codec::DEFAULT_SECOND_COLORS;
    let mut ledger = ShadowLedger::default();
    ledger.append(ShadowEntry::new(7, 0x0038, 0x0740, "1000")); // 50% brightness

    let plans = panteon_ms89::service::plan_color_session(&main_rb, &second_rb, &ledger, None)
        .expect("plan_color_session from ledger should succeed");
    assert_eq!(plans.len(), 32);

    let b7_plan = plans.iter().find(|p| p.addr == 0x0038 && p.bank == 7).expect("bank 7 plan must exist");
    assert_eq!(b7_plan.data, 0x0740);
}

#[test]
fn color_write_empty_ledger_no_override_rejected() {
    let main_rb: [(u8, u8, u8); 6] = [
        (255, 0, 0), (0, 0, 255), (255, 0, 0),
        (0, 0, 255), (255, 0, 0), (0, 0, 255),
    ];
    let second_rb = panteon_ms89::codec::DEFAULT_SECOND_COLORS;
    let ledger = ShadowLedger::default();

    let res = panteon_ms89::service::plan_color_session(&main_rb, &second_rb, &ledger, None);
    assert!(res.is_err(), "Empty ledger without override must be rejected");
    match res.unwrap_err() {
        PanteonError::Protocol(msg) => {
            assert_eq!(msg, "LED brightness unknown, supply --brightness / set in GUI");
        }
        other => panic!("Expected Protocol error, got: {:?}", other),
    }
}

#[test]
fn color_write_user_override_wins() {
    let main_rb: [(u8, u8, u8); 6] = [
        (255, 0, 0), (0, 0, 255), (255, 0, 0),
        (0, 0, 255), (255, 0, 0), (0, 0, 255),
    ];
    let second_rb = panteon_ms89::codec::DEFAULT_SECOND_COLORS;
    let mut ledger = ShadowLedger::default();
    ledger.append(ShadowEntry::new(7, 0x0038, 0x0740, "1000")); // 50% in ledger

    // User supplies override: 2 (25% -> 0x0760)
    let plans = panteon_ms89::service::plan_color_session(&main_rb, &second_rb, &ledger, Some(2))
        .expect("plan_color_session with override should succeed");
    let b7_plan = plans.iter().find(|p| p.addr == 0x0038 && p.bank == 7).expect("bank 7 plan must exist");
    assert_eq!(b7_plan.data, 0x0760);
}

#[test]
fn test_execute_led_brightness_records_to_ledger() {
    let verified = DeviceIdentity {
        pid: 0x1440,
        protocol_verified: true,
        accepted_signatures: &[0x0976, 0x0978],
    };
    let mut transport = MockGateTransport::with_identity(WriteGateLevel::Level2ConfigurationWrite, verified);
    let mut ledger = ShadowLedger::default();

    // Execute brightness 2 (25% -> 0x0760)
    panteon_ms89::service::execute_led_brightness(&mut transport, 2, &mut ledger, false, false, 0)
        .expect("execute_led_brightness should succeed");

    // Check ledger
    assert_eq!(ledger.entries.len(), 1);
    assert_eq!(ledger.entries[0].bank, 7);
    assert_eq!(ledger.entries[0].addr, 0x0038);
    assert_eq!(ledger.entries[0].data, 0x0760);

    // Verify wire frame for Bank 7 @ 0x0038 is 0x0760
    assert_eq!(transport.raw_writes.len(), 17);
    assert_eq!(transport.raw_writes[10], [0x07, 0x18, 0x03, 0x07, 0x38, 0x00, 0x60, 0x07]);
}



