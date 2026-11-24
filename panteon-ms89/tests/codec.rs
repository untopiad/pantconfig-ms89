use panteon_ms89::codec::{
    decode_command, decode_descriptor_page, decode_get_report_response, decode_packed_mem,
    decode_profile_block, decode_read_response, decode_value16, encode_0x17, encode_command,
    encode_get, encode_packed_mem, encode_page_base, encode_probe, encode_profile_block,
    encode_read_arm_finalize, encode_read_bank_arm_finalize, encode_read_bank_frame,
    encode_read_bank_prep_frame, encode_read_prep_frame, encode_read_request,
    encode_read_request_0x13, encode_read_request_mode, encode_read_reset,
    encode_read_reset_frame, encode_read_sweep_frame, encode_value16, Command,
    DeviceGetReportResponse, ReadPhase, ReadSequenceFrame, HYPOTHETICAL_PAGE_BASE_PROBE, LIVE_DESCRIPTOR_PAGE_BYTES,
    POST_WRITE_DESCRIPTOR_PAGE_BYTES, READ_REQUEST_MODE, READ_REQUEST_MODE_08_REFUTED,
};
use panteon_ms89::device::Transport;
use panteon_ms89::model::{registers, DeviceState, PackedMemLayout, PackedMemMode, PollingRate};
use panteon_ms89::{FEATURE_REPORT_LEN, REPORT_ID};

#[test]
fn encode_0x17_be() {
    // 1234 == 0x04D2 in big-endian
    let report = encode_0x17(1234);
    assert_eq!(report.len(), FEATURE_REPORT_LEN);
    assert_eq!(report[0], 0x07);
    assert_eq!(report[1], 0x17);
    assert_eq!(report[2], 0x04);
    assert_eq!(report[3], 0xD2);
    // Verified 8-byte prefix: 07 17 04 D2 00 00 00 00
    assert_eq!(&report[..8], &[0x07, 0x17, 0x04, 0xD2, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(report, [0x07, 0x17, 0x04, 0xD2, 0x00, 0x00, 0x00, 0x00]);

    // Test decode roundtrip
    let decoded = decode_value16(&report).expect("decode_value16 should succeed");
    assert_eq!(decoded, 1234);
}

#[test]
fn probe_command() {
    let report = encode_probe();
    assert_eq!(report.len(), FEATURE_REPORT_LEN);
    assert_eq!(report[0], 0x07);
    assert_eq!(report[1], 0x10);
    assert_eq!(report[..2], [0x07, 0x10]);

    let cmd = decode_command(&report).expect("decode_command should succeed");
    assert_eq!(cmd, Command::Probe);
}

#[test]
fn page_base() {
    let report = encode_page_base();
    assert_eq!(report.len(), FEATURE_REPORT_LEN);
    assert_eq!(report[0], 0x07);
    assert_eq!(report[1], 0x13);
    // Hypothesis: 07 13 00 80 00 40 00 00
    assert_eq!(&report[..8], &[0x07, 0x13, 0x00, 0x80, 0x00, 0x40, 0x00, 0x00]);
    assert_eq!(report, [0x07, 0x13, 0x00, 0x80, 0x00, 0x40, 0x00, 0x00]);

    let cmd = decode_command(&report).expect("decode_command should succeed");
    assert_eq!(cmd, Command::PageBase);
}

#[test]
fn trace_vector_polling_500hz() {
    // Trace vector: ctrl_a_open -> 500 Hz
    let raw: [u8; FEATURE_REPORT_LEN] = [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x01, 0x07];
    let decoded = decode_packed_mem(&raw).expect("decode polling 500Hz vector");
    assert_eq!(decoded.mode, 0x03);
    assert_eq!(decoded.parsed_mode().unwrap(), PackedMemMode::Write);
    assert_eq!(decoded.bank, registers::POLLING_RATE.0);
    assert_eq!(decoded.addr, registers::POLLING_RATE.1);
    assert_eq!(decoded.addr, 0x0018);
    assert_eq!(decoded.data, 0x0701);

    // Roundtrip pack -> raw
    let encoded = encode_packed_mem(&decoded);
    assert_eq!(encoded, raw);
}

#[test]
fn trace_vector_polling_1000hz() {
    // Trace vector: ctrl_f_polling_500to1000 -> 1000 Hz
    let raw: [u8; FEATURE_REPORT_LEN] = [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x00, 0x07];
    let decoded = decode_packed_mem(&raw).expect("decode polling 1000Hz vector");
    assert_eq!(decoded.mode, 0x03);
    assert_eq!(decoded.parsed_mode().unwrap(), PackedMemMode::Write);
    assert_eq!(decoded.bank, registers::POLLING_RATE.0);
    assert_eq!(decoded.addr, registers::POLLING_RATE.1);
    assert_eq!(decoded.addr, 0x0018);
    assert_eq!(decoded.data, 0x0700);
    assert_eq!(decoded.data, registers::polling_rate_codes::HZ_1000);

    // Roundtrip pack -> raw
    let encoded = encode_packed_mem(&decoded);
    assert_eq!(encoded, raw);
}

#[test]
fn trace_vector_polling_125hz() {
    // Trace vector: ctrl_m_poll_125 -> 125 Hz
    let raw: [u8; FEATURE_REPORT_LEN] = [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x07, 0x07];
    let decoded = decode_packed_mem(&raw).expect("decode polling 125Hz vector");
    assert_eq!(decoded.mode, 0x03);
    assert_eq!(decoded.parsed_mode().unwrap(), PackedMemMode::Write);
    assert_eq!(decoded.bank, registers::POLLING_RATE.0);
    assert_eq!(decoded.addr, registers::POLLING_RATE.1);
    assert_eq!(decoded.addr, 0x0018);
    assert_eq!(decoded.data, 0x0707);
    assert_eq!(decoded.data, registers::polling_rate_codes::HZ_125);

    // Roundtrip pack -> raw
    let encoded = encode_packed_mem(&decoded);
    assert_eq!(encoded, raw);
}

#[test]
fn polling_rate_model_mapping() {
    use panteon_ms89::model::PollingRate;

    assert_eq!(PollingRate::Hz1000.to_register_code(), registers::polling_rate_codes::HZ_1000);
    assert_eq!(PollingRate::Hz500.to_register_code(), registers::polling_rate_codes::HZ_500);
    assert_eq!(PollingRate::Hz250.to_register_code(), registers::polling_rate_codes::HZ_250);
    assert_eq!(PollingRate::Hz125.to_register_code(), registers::polling_rate_codes::HZ_125);

    assert_eq!(PollingRate::from_register_code(0x0700), Some(PollingRate::Hz1000));
    assert_eq!(PollingRate::from_register_code(0x0701), Some(PollingRate::Hz500));
    assert_eq!(PollingRate::from_register_code(0x0703), Some(PollingRate::Hz250));
    assert_eq!(PollingRate::from_register_code(0x0707), Some(PollingRate::Hz125));
    assert_eq!(PollingRate::from_register_code(0x07ff), None);
}

#[test]
fn trace_vector_dpi_slot2() {
    // Trace vector: ctrl_h_slot2_800 -> Slot 2 (0x0719)
    let raw: [u8; FEATURE_REPORT_LEN] = [0x07, 0x18, 0x03, 0x00, 0x18, 0x00, 0x19, 0x07];
    let decoded = decode_packed_mem(&raw).expect("decode DPI slot 2 vector");
    assert_eq!(decoded.mode, 0x03);
    assert_eq!(decoded.parsed_mode().unwrap(), PackedMemMode::Write);
    assert_eq!(decoded.bank, registers::ACTIVE_DPI_SLOT.0);
    assert_eq!(decoded.addr, registers::ACTIVE_DPI_SLOT.1);
    assert_eq!(decoded.addr, 0x0018);
    assert_eq!(decoded.data, registers::dpi_slot_codes::SLOT_2);
    assert_eq!(decoded.data, 0x0719);

    let encoded = encode_packed_mem(&decoded);
    assert_eq!(encoded, raw);
}

#[test]
fn trace_vector_dpi_slot4() {
    // Trace vector: ctrl_j_slot4_3200 -> Slot 4 (0x071b)
    let raw: [u8; FEATURE_REPORT_LEN] = [0x07, 0x18, 0x03, 0x00, 0x18, 0x00, 0x1b, 0x07];
    let decoded = decode_packed_mem(&raw).expect("decode DPI slot 4 vector");
    assert_eq!(decoded.mode, 0x03);
    assert_eq!(decoded.parsed_mode().unwrap(), PackedMemMode::Write);
    assert_eq!(decoded.bank, registers::ACTIVE_DPI_SLOT.0);
    assert_eq!(decoded.addr, registers::ACTIVE_DPI_SLOT.1);
    assert_eq!(decoded.addr, 0x0018);
    assert_eq!(decoded.data, registers::dpi_slot_codes::SLOT_4);
    assert_eq!(decoded.data, 0x071b);

    let encoded = encode_packed_mem(&decoded);
    assert_eq!(encoded, raw);
}

#[test]
fn trace_vector_dpi_slot6() {
    // Trace vector: ctrl_l_slot6_4800 -> Slot 6 (0x071d)
    let raw: [u8; FEATURE_REPORT_LEN] = [0x07, 0x18, 0x03, 0x00, 0x18, 0x00, 0x1d, 0x07];
    let decoded = decode_packed_mem(&raw).expect("decode DPI slot 6 vector");
    assert_eq!(decoded.mode, 0x03);
    assert_eq!(decoded.parsed_mode().unwrap(), PackedMemMode::Write);
    assert_eq!(decoded.bank, registers::ACTIVE_DPI_SLOT.0);
    assert_eq!(decoded.addr, registers::ACTIVE_DPI_SLOT.1);
    assert_eq!(decoded.addr, 0x0018);
    assert_eq!(decoded.data, registers::dpi_slot_codes::SLOT_6);
    assert_eq!(decoded.data, 0x071d);

    let encoded = encode_packed_mem(&decoded);
    assert_eq!(encoded, raw);
}

#[test]
fn trace_vector_led() {
    // Trace vector: ctrl_a_open / ctrl_e_led
    let raw: [u8; FEATURE_REPORT_LEN] = [0x07, 0x18, 0x03, 0x04, 0x18, 0x00, 0xf3, 0x07];
    let decoded = decode_packed_mem(&raw).expect("decode LED vector");
    assert_eq!(decoded.mode, 0x03);
    assert_eq!(decoded.parsed_mode().unwrap(), PackedMemMode::Write);
    assert_eq!(decoded.bank, registers::LED_EFFECT.0);
    assert_eq!(decoded.addr, registers::LED_EFFECT.1);
    assert_eq!(decoded.addr, 0x0018);
    assert_eq!(decoded.data, 0x07f3);

    // Roundtrip pack -> raw
    let encoded = encode_packed_mem(&decoded);
    assert_eq!(encoded, raw);
}

#[test]
fn trace_vector_dpi() {
    // Trace vector: ctrl_a_open / ctrl_b_dpi_200_400
    let raw: [u8; FEATURE_REPORT_LEN] = [0x07, 0x18, 0x03, 0x05, 0x10, 0x00, 0x12, 0x07];
    let decoded = decode_packed_mem(&raw).expect("decode DPI vector");
    assert_eq!(decoded.mode, 0x03);
    assert_eq!(decoded.parsed_mode().unwrap(), PackedMemMode::Write);
    assert_eq!(decoded.bank, registers::DPI_CONFIG.0);
    assert_eq!(decoded.addr, registers::DPI_CONFIG.1);
    assert_eq!(decoded.addr, 0x0010);
    assert_eq!(decoded.data, 0x0712);

    // Roundtrip pack -> raw
    let encoded = encode_packed_mem(&decoded);
    assert_eq!(encoded, raw);
}

#[test]
fn trace_vector_bank_commit_and_final_commit() {
    // Bank commit (mode 0x09)
    let raw_commit: [u8; FEATURE_REPORT_LEN] = [0x07, 0x18, 0x09, 0x00, 0x70, 0x00, 0x00, 0x07];
    let decoded_commit = decode_packed_mem(&raw_commit).expect("decode bank commit vector");
    assert_eq!(decoded_commit.mode, 0x09);
    assert_eq!(decoded_commit.parsed_mode().unwrap(), PackedMemMode::CommitBank);
    assert_eq!(decoded_commit.bank, 0);
    assert_eq!(decoded_commit.addr, 0x0070);
    assert_eq!(decoded_commit.data, 0x0700);
    assert_eq!(encode_packed_mem(&decoded_commit), raw_commit);

    // Final transaction commit (mode 0x00)
    let raw_final: [u8; FEATURE_REPORT_LEN] = [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let decoded_final = decode_packed_mem(&raw_final).expect("decode final commit vector");
    assert_eq!(decoded_final.mode, 0x00);
    assert_eq!(decoded_final.parsed_mode().unwrap(), PackedMemMode::FinalCommit);
    assert_eq!(encode_packed_mem(&decoded_final), raw_final);
}

#[test]
fn packed_mem_negative_validations() {
    // 1. Invalid report ID (not 0x07)
    let bad_report_id: [u8; FEATURE_REPORT_LEN] = [0x06, 0x18, 0x03, 0x06, 0x18, 0x00, 0x01, 0x07];
    let err = decode_packed_mem(&bad_report_id).expect_err("should fail with bad report ID");
    assert_eq!(err, panteon_ms89::PanteonError::Protocol("Invalid report ID"));

    // 2. Invalid command byte (not 0x18)
    let bad_command: [u8; FEATURE_REPORT_LEN] = [0x07, 0x19, 0x03, 0x06, 0x18, 0x00, 0x01, 0x07];
    let err = decode_packed_mem(&bad_command).expect_err("should fail with bad command byte");
    assert_eq!(err, panteon_ms89::PanteonError::Protocol("Invalid command byte for PackedMem"));

    // 3. Invalid mode byte (0x05 not in {0x00, 0x03, 0x09})
    let bad_mode: [u8; FEATURE_REPORT_LEN] = [0x07, 0x18, 0x05, 0x06, 0x18, 0x00, 0x01, 0x07];
    let err = decode_packed_mem(&bad_mode).expect_err("should fail with bad mode byte");
    assert_eq!(err, panteon_ms89::PanteonError::Protocol("Invalid PackedMemMode byte"));
}

#[test]
fn report_length_invariant() {
    let reports: Vec<[u8; FEATURE_REPORT_LEN]> = vec![
        encode_probe(),
        encode_get(),
        encode_page_base(),
        encode_profile_block(1, 0, 1600),
        encode_value16(0),
        encode_value16(65535),
        encode_packed_mem(&PackedMemLayout {
            mode: 0x03,
            bank: 0,
            addr: 0x0018,
            data: 0x0700,
        }),
        encode_command(Command::Unknown15),
        encode_command(Command::Unknown16),
        encode_command(Command::Unknown20),
    ];

    for report in reports {
        assert_eq!(report.len(), FEATURE_REPORT_LEN, "Every encoded report must be exactly 8 bytes");
        assert_eq!(report[0], 0x07, "Report ID must be 0x07");
    }
}

#[test]
fn profile_block_roundtrip() {
    let report = encode_profile_block(2, 1, 2400);
    assert_eq!(report.len(), FEATURE_REPORT_LEN);
    assert_eq!(report[0], REPORT_ID);
    assert_eq!(report[1], Command::ProfileBlock as u8);

    let (profile_bank, flag, threshold) =
        decode_profile_block(&report).expect("decode_profile_block should succeed");
    assert_eq!(profile_bank, 2);
    assert_eq!(flag, 1);
    assert_eq!(threshold, 2400);
}

#[test]
fn device_state_from_synthetic_trace_vectors() {
    // Vector 1: Slot 1 (0x0718) + Polling 1000 Hz (0x0700)
    let entries_1 = vec![
        (registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1, 0x0718),
        (registers::POLLING_RATE.0, registers::POLLING_RATE.1, 0x0700),
    ];
    let state_1 = DeviceState::from_registers(&entries_1).expect("valid state for slot 1 / 1000Hz");
    assert_eq!(state_1.confidence, panteon_ms89::StateConfidence::RegistersConfirmed);
    assert_eq!(state_1.active_dpi_slot, Some(1));
    assert_eq!(state_1.raw_slot_value, Some(0x0718));
    assert_eq!(state_1.polling_rate, Some(PollingRate::Hz1000));
    assert_eq!(state_1.raw_polling_value, Some(0x0700));

    // Vector 2: Slot 2 (0x0719) + Polling 500 Hz (0x0701)
    let entries_2 = vec![
        (registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1, 0x0719),
        (registers::POLLING_RATE.0, registers::POLLING_RATE.1, 0x0701),
    ];
    let state_2 = DeviceState::from_registers(&entries_2).expect("valid state for slot 2 / 500Hz");
    assert_eq!(state_2.confidence, panteon_ms89::StateConfidence::RegistersConfirmed);
    assert_eq!(state_2.active_dpi_slot, Some(2));
    assert_eq!(state_2.polling_rate, Some(PollingRate::Hz500));

    // Vector 3: Slot 3 (0x071a) + Polling 250 Hz (0x0703)
    let entries_3 = vec![
        (registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1, 0x071a),
        (registers::POLLING_RATE.0, registers::POLLING_RATE.1, 0x0703),
    ];
    let state_3 = DeviceState::from_registers(&entries_3).expect("valid state for slot 3 / 250Hz");
    assert_eq!(state_3.confidence, panteon_ms89::StateConfidence::RegistersConfirmed);
    assert_eq!(state_3.active_dpi_slot, Some(3));
    assert_eq!(state_3.polling_rate, Some(PollingRate::Hz250));

    // Vector 4: Slot 6 (0x071d) + Polling 125 Hz (0x0707)
    let entries_4 = vec![
        (registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1, 0x071d),
        (registers::POLLING_RATE.0, registers::POLLING_RATE.1, 0x0707),
    ];
    let state_4 = DeviceState::from_registers(&entries_4).expect("valid state for slot 6 / 125Hz");
    assert_eq!(state_4.confidence, panteon_ms89::StateConfidence::RegistersConfirmed);
    assert_eq!(state_4.active_dpi_slot, Some(6));
    assert_eq!(state_4.polling_rate, Some(PollingRate::Hz125));
}

#[test]
fn device_state_from_packed_reports_trace_vectors() {
    // Reports corresponding to Slot 4 (0x071b) and Polling 500 Hz (0x0701)
    let slot_report = [0x07, 0x18, 0x03, 0x00, 0x18, 0x00, 0x1b, 0x07];
    let poll_report = [0x07, 0x18, 0x03, 0x06, 0x18, 0x00, 0x01, 0x07];

    let state = DeviceState::from_packed_reports(&[slot_report, poll_report])
        .expect("should decode DeviceState from packed reports");
    assert_eq!(state.confidence, panteon_ms89::StateConfidence::RegistersConfirmed);
    assert_eq!(state.active_dpi_slot, Some(4));
    assert_eq!(state.polling_rate, Some(PollingRate::Hz500));
    assert_eq!(state.raw_slot_value, Some(0x071b));
    assert_eq!(state.raw_polling_value, Some(0x0701));
}

#[test]
fn device_state_validation_invalid_slot() {
    // 1. Slot index 0 -> Err
    assert!(DeviceState::slot_index_to_raw(0).is_err());

    // 2. Slot index 7 -> Err (out of bounds 1..=6)
    assert!(DeviceState::slot_index_to_raw(7).is_err());

    // 3. Raw slot value 0x0717 (slot 0) -> Err
    assert!(DeviceState::raw_to_slot_index(0x0717).is_err());

    // 4. Raw slot value 0x071e (slot 7) -> Err
    assert!(DeviceState::raw_to_slot_index(0x071e).is_err());

    // 5. Raw slot value with corrupted high byte (0x0818) -> Err
    assert!(DeviceState::raw_to_slot_index(0x0818).is_err());

    // 6. DeviceState::new(7, ...) -> Err
    assert!(DeviceState::new(7, PollingRate::Hz1000).is_err());
}

#[test]
fn device_state_validation_invalid_polling() {
    // Unknown polling code (e.g. 0x07ff)
    let bad_entries = vec![
        (registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1, 0x0718),
        (registers::POLLING_RATE.0, registers::POLLING_RATE.1, 0x07ff),
    ];
    let err = DeviceState::from_registers(&bad_entries).expect_err("unknown polling code must fail");
    assert_eq!(
        err,
        panteon_ms89::PanteonError::Protocol("Invalid or unknown polling rate register value")
    );
}

#[test]
fn device_state_validation_missing_registers() {
    // Missing polling rate register
    let only_slot = vec![
        (registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1, 0x0718),
    ];
    let err = DeviceState::from_registers(&only_slot).expect_err("missing polling register must fail");
    assert_eq!(
        err,
        panteon_ms89::PanteonError::Protocol("Missing POLLING_RATE (Bank 6, Addr 0x0018) register in state read")
    );

    // Missing slot register
    let only_polling = vec![
        (registers::POLLING_RATE.0, registers::POLLING_RATE.1, 0x0700),
    ];
    let err = DeviceState::from_registers(&only_polling).expect_err("missing slot register must fail");
    assert_eq!(
        err,
        panteon_ms89::PanteonError::Protocol("Missing ACTIVE_DPI_SLOT (Bank 0, Addr 0x0018) register in state read")
    );
}

#[test]
fn device_get_report_response_decode_live_traces() {
    // Live trace GET_REPORT response: ctrl_a_open
    let report_a: [u8; FEATURE_REPORT_LEN] = [0x07, 0x00, 0x76, 0x09, 0x3c, 0x00, 0x23, 0x7a];
    let resp_a = decode_get_report_response(&report_a).expect("decode ctrl_a GET_REPORT response");
    assert_eq!(resp_a.report_id, 0x07);
    assert_eq!(resp_a.command, 0x00);
    assert_eq!(resp_a.signature, 0x0976);
    assert_eq!(resp_a.revision, 0x3c);
    assert_eq!(resp_a.status, 0x23);
    assert_eq!(resp_a.flags, 0x7a);

    // Decode state from handshake frame -> HandshakeOnly with unconfirmed slot/polling
    let state = DeviceState::from_feature_report(&report_a).expect("decode DeviceState from handshake report");
    assert_eq!(state.confidence, panteon_ms89::StateConfidence::HandshakeOnly);
    assert_eq!(state.active_dpi_slot, None);
    assert_eq!(state.polling_rate, None);
    assert!(state.handshake.is_some());

    // Live trace GET_REPORT response: mouse_packets
    let report_mp: [u8; FEATURE_REPORT_LEN] = [0x07, 0x00, 0x76, 0x09, 0x51, 0x00, 0x23, 0x0a];
    let resp_mp = decode_get_report_response(&report_mp).expect("decode mouse_packets response");
    assert_eq!(resp_mp.revision, 0x51);

    // Negative: invalid report ID
    let bad_id: [u8; FEATURE_REPORT_LEN] = [0x08, 0x00, 0x76, 0x09, 0x3c, 0x00, 0x23, 0x7a];
    assert!(decode_get_report_response(&bad_id).is_err());

    // Negative: invalid signature
    let bad_sig: [u8; FEATURE_REPORT_LEN] = [0x07, 0x00, 0x76, 0x08, 0x3c, 0x00, 0x23, 0x7a];
    assert!(decode_get_report_response(&bad_sig).is_err());
}

#[test]
fn decode_get_report_response_with_expected_signatures() {
    let buf_0976 = [0x07, 0x00, 0x76, 0x09, 0x51, 0x00, 0x23, 0x7a];
    let buf_0978 = [0x07, 0x00, 0x78, 0x09, 0x52, 0x00, 0x23, 0x7b];

    // Correct signatures match
    let resp_76 = DeviceGetReportResponse::decode_with_expected_signature(&buf_0976, 0x0976).unwrap();
    assert_eq!(resp_76.signature, 0x0976);
    assert_eq!(resp_76.revision, 0x51);

    let resp_78 = DeviceGetReportResponse::decode_with_expected_signature(&buf_0978, 0x0978).unwrap();
    assert_eq!(resp_78.signature, 0x0978);
    assert_eq!(resp_78.revision, 0x52);

    // Cross signatures fail
    assert!(DeviceGetReportResponse::decode_with_expected_signature(&buf_0976, 0x0978).is_err());
    assert!(DeviceGetReportResponse::decode_with_expected_signature(&buf_0978, 0x0976).is_err());
}

struct MockStateTransport {
    report_to_return: [u8; FEATURE_REPORT_LEN],
}

impl Transport for MockStateTransport {
    fn read_feature(&mut self, report: &mut [u8; FEATURE_REPORT_LEN]) -> Result<(), panteon_ms89::PanteonError> {
        *report = self.report_to_return;
        Ok(())
    }
}

#[test]
fn transport_read_state_mock() {
    let mut transport = MockStateTransport {
        report_to_return: [0x07, 0x00, 0x76, 0x09, 0x3c, 0x00, 0x23, 0x5a],
    };
    let state = transport.read_state().expect("read_state via transport should succeed");
    assert_eq!(state.confidence, panteon_ms89::StateConfidence::HandshakeOnly);
    assert_eq!(state.active_dpi_slot, None);
    assert_eq!(state.polling_rate, None);
    let hs = state.handshake.expect("handshake response present");
    assert_eq!(hs.signature, 0x0976);
    assert_eq!(hs.revision, 0x3c);
    assert_eq!(hs.status, 0x23);
    assert_eq!(hs.flags, 0x5a);
}

#[test]
fn read_response_truncated_negative() {
    let err_empty = decode_read_response(&[]).expect_err("empty buffer must fail");
    match err_empty {
        panteon_ms89::PanteonError::ProtocolDetailed { context, detail } => {
            assert_eq!(context, "decode_handshake");
            assert!(detail.contains("Truncated"));
            assert!(detail.contains("raw=[]"));
        }
        other => panic!("Expected ProtocolDetailed, got: {:?}", other),
    }

    let short = [0x07, 0x00, 0x76, 0x09, 0x3c, 0x00, 0x23];
    let err_short = decode_read_response(&short).expect_err("7-byte buffer must fail");
    match err_short {
        panteon_ms89::PanteonError::ProtocolDetailed { context, detail } => {
            assert_eq!(context, "decode_handshake");
            assert!(detail.contains("Truncated"));
            assert!(detail.contains("raw=[07 00 76 09 3c 00 23]"));
        }
        other => panic!("Expected ProtocolDetailed, got: {:?}", other),
    }
}

#[test]
fn read_response_invalid_report_id_negative() {
    let bad_id = [0x08, 0x00, 0x76, 0x09, 0x3c, 0x00, 0x23, 0x7a];
    let err = decode_read_response(&bad_id).expect_err("invalid report id must fail");
    match err {
        panteon_ms89::PanteonError::ProtocolDetailed { context, detail } => {
            assert_eq!(context, "decode_handshake");
            assert!(detail.contains("Invalid report ID"));
            assert!(detail.contains("raw=[08 00 76 09 3c 00 23 7a]"));
        }
        other => panic!("Expected ProtocolDetailed, got: {:?}", other),
    }
}

#[test]
fn read_response_live_trace_vectors() {
    // ctrl_h_slot2_800 live vector (8 bytes: slot 2, 1000 Hz)
    let trace_h = [0x07, 0x00, 0x76, 0x09, 0x3c, 0x00, 0x03, 0x5a];
    let resp_h = decode_read_response(&trace_h).expect("decode ctrl_h read response");
    assert_eq!(resp_h.revision, 0x3c);
    assert_eq!(resp_h.status, 0x03);
    assert_eq!(resp_h.flags, 0x5a);

    // ctrl_m_poll_125 live vector
    let trace_m = [0x07, 0x00, 0x76, 0x09, 0x3c, 0x00, 0x03, 0x5a, 0x00];
    let resp_m = decode_read_response(&trace_m).expect("decode ctrl_m read response");
    assert_eq!(resp_m.revision, 0x3c);

    // mouse_packets live vector with revision 0x51
    let trace_mp = [0x07, 0x00, 0x76, 0x09, 0x51, 0x00, 0x23, 0x0a];
    let resp_mp = decode_read_response(&trace_mp).expect("decode mouse_packets read response");
    assert_eq!(resp_mp.revision, 0x51);
    assert_eq!(resp_mp.flags, 0x0a);
}

#[test]
fn handshake_golden_vectors_volatile_state_bytes_with_signature() {
    let test_states = [0x00u8, 0xdf, 0xff, 0x03];
    for state in test_states {
        let raw = [0x07, state, 0x76, 0x09, 0x3c, 0x00, 0x23, 0x7a];
        let resp = DeviceGetReportResponse::decode_with_expected_signature(&raw, 0x0976)
            .unwrap_or_else(|e| panic!("state 0x{:02x} must decode cleanly: {}", state, e));
        assert_eq!(resp.report_id, 0x07);
        assert_eq!(resp.state_byte, state);
        assert_eq!(resp.signature, 0x0976);
        assert_eq!(resp.revision, 0x3c);
        assert_eq!(resp.status, 0x23);
        assert_eq!(resp.flags, 0x7a);
    }
}

#[test]
fn handshake_negative_signature_not_in_accepted() {
    let raw = [0x07, 0xff, 0x34, 0x12, 0x3c, 0x00, 0x23, 0x7a];
    let err = DeviceGetReportResponse::decode_with_expected_signatures(&raw, &[0x0976, 0x0978])
        .expect_err("signature 0x1234 must be rejected");
    match err {
        panteon_ms89::PanteonError::ProtocolDetailed { context, detail } => {
            assert_eq!(context, "decode_handshake");
            assert!(detail.contains("0x1234"));
            assert!(detail.contains("raw=[07 ff 34 12 3c 00 23 7a]"));
            assert!(detail.contains("0x0976"));
            assert!(detail.contains("0x0978"));
        }
        other => panic!("Expected ProtocolDetailed, got: {:?}", other),
    }
}

struct MockConfirmedTransport;

impl Transport for MockConfirmedTransport {
    fn read_feature(&mut self, _report: &mut [u8; FEATURE_REPORT_LEN]) -> Result<(), panteon_ms89::PanteonError> {
        Ok(())
    }

    fn read_state(&mut self) -> Result<DeviceState, panteon_ms89::PanteonError> {
        DeviceState::new(3, PollingRate::Hz1000)
    }
}

#[test]
fn transport_read_register_confirmed_and_unconfirmed() {
    let mut transport = MockConfirmedTransport;

    // Confirmed ACTIVE_DPI_SLOT register (Bank 0, Addr 0x0018)
    let slot_val = transport
        .read_register(registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1)
        .expect("read confirmed ACTIVE_DPI_SLOT register");
    assert_eq!(slot_val, 0x071a); // Slot 3 raw code

    // Confirmed POLLING_RATE register (Bank 6, Addr 0x0018)
    let poll_val = transport
        .read_register(registers::POLLING_RATE.0, registers::POLLING_RATE.1)
        .expect("read confirmed POLLING_RATE register");
    assert_eq!(poll_val, 0x0700); // 1000 Hz raw code

    // Unconfirmed arbitrary register (Bank 4, Addr 0x0018 - LED)
    let err = transport
        .read_register(4, 0x0018)
        .expect_err("arbitrary register read must fail without hardware write");
    assert_eq!(
        err,
        panteon_ms89::PanteonError::Protocol(
            "Arbitrary register read is not supported by device firmware without hardware write (blocked by Write-Gate)"
        )
    );

    // Handshake-only transport must fail read_register even for slot/polling
    let mut hs_transport = MockStateTransport {
        report_to_return: [0x07, 0x00, 0x76, 0x09, 0x3c, 0x00, 0x03, 0x5a],
    };
    let hs_err = hs_transport
        .read_register(registers::ACTIVE_DPI_SLOT.0, registers::ACTIVE_DPI_SLOT.1)
        .expect_err("handshake-only transport must reject register read");
    assert_eq!(
        hs_err,
        panteon_ms89::PanteonError::Protocol(
            "Arbitrary register read is not supported by device firmware without hardware write (blocked by Write-Gate)"
        )
    );
}

#[test]
fn handshake_same_across_different_slots_produces_handshake_only() {
    // ctrl_h_slot2 (known Slot 2 from memmap)
    let report_h: [u8; FEATURE_REPORT_LEN] = [0x07, 0x00, 0x76, 0x09, 0x3c, 0x00, 0x03, 0x5a];
    let state_h = DeviceState::from_feature_report(&report_h).expect("decode ctrl_h handshake");
    assert_eq!(state_h.confidence, panteon_ms89::StateConfidence::HandshakeOnly);
    assert_eq!(state_h.active_dpi_slot, None);
    assert_eq!(state_h.polling_rate, None);
    assert_eq!(state_h.handshake.unwrap().status, 0x03);
    assert_eq!(state_h.handshake.unwrap().flags, 0x5a);

    // ctrl_l_slot6 (known Slot 6 from memmap) - IDENTICAL handshake bytes as Slot 2!
    let report_l: [u8; FEATURE_REPORT_LEN] = [0x07, 0x00, 0x76, 0x09, 0x3c, 0x00, 0x03, 0x5a];
    let state_l = DeviceState::from_feature_report(&report_l).expect("decode ctrl_l handshake");
    assert_eq!(state_l.confidence, panteon_ms89::StateConfidence::HandshakeOnly);
    assert_eq!(state_l.active_dpi_slot, None);
    assert_eq!(state_l.polling_rate, None);
    assert_eq!(state_l.handshake.unwrap().status, 0x03);
    assert_eq!(state_l.handshake.unwrap().flags, 0x5a);

    // Both sessions yield identical handshake frames despite having completely different DPI slots (2 vs 6)
    assert_eq!(state_h, state_l);
}

#[test]
fn encode_read_request_mode_05_and_sweep() {
    assert_eq!(READ_REQUEST_MODE, 0x05);
    assert_eq!(READ_REQUEST_MODE_08_REFUTED, 0x08);

    // Bank 5, Addr 0x0010
    let req = encode_read_request(5, 0x0010);
    assert_eq!(req.len(), FEATURE_REPORT_LEN);
    assert_eq!(req[0], 0x07);
    assert_eq!(req[1], 0x18);
    assert_eq!(req[2], 0x05); // mode 0x05 (protocol-probed)
    assert_eq!(req[3], 0x05); // Bank 5
    assert_eq!(req[4], 0x10); // Addr LE lo
    assert_eq!(req[5], 0x00); // Addr LE hi
    assert_eq!(req[6], 0x00);
    assert_eq!(req[7], 0x00);
    assert_eq!(req, [0x07, 0x18, 0x05, 0x05, 0x10, 0x00, 0x00, 0x00]);

    // Ground truth target: POLLING_RATE (Bank 6, Addr 0x0018)
    let req_poll = encode_read_request(registers::POLLING_RATE.0, registers::POLLING_RATE.1);
    assert_eq!(req_poll, [0x07, 0x18, 0x05, 0x06, 0x18, 0x00, 0x00, 0x00]);

    // Read reset frame: 07 18 05 00 00 00 00 00
    let reset = encode_read_reset();
    assert_eq!(reset, [0x07, 0x18, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // Test sweep frame encoding: 07 18 05 <i> <addr_lo> <addr_hi> 00 <count_minus_1>
    let sweep_frame = encode_read_sweep_frame(0, 0x0123, 0);
    assert_eq!(
        sweep_frame,
        [0x07, 0x18, 0x05, 0x00, 0x23, 0x01, 0x00, 0x00]
    );

    let sweep_frame_chunk = encode_read_sweep_frame(3, 0x0200, 7);
    assert_eq!(
        sweep_frame_chunk,
        [0x07, 0x18, 0x05, 0x03, 0x00, 0x02, 0x00, 0x07]
    );

    // Test custom mode encoder (mode 0x02)
    let req_m2 = encode_read_request_mode(0x02, 6, 0x0018);
    assert_eq!(req_m2[1], 0x18);
    assert_eq!(req_m2[2], 0x02);
    assert_eq!(req_m2[3], 0x06);

    // Test candidate PageBase 0x13 probe format
    assert_eq!(
        HYPOTHETICAL_PAGE_BASE_PROBE,
        [0x07, 0x13, 0x00, 0x80, 0x00, 0x40, 0x00, 0x00]
    );

    let req_13 = encode_read_request_0x13(0x0080, 0x0040);
    assert_eq!(req_13, HYPOTHETICAL_PAGE_BASE_PROBE);
}

#[test]
fn encode_read_bank_frames_vector() {
    // Target: Bank 6, Addr 0x0018 (polling rate), Count 4 -> count_minus_1 = 3
    let prep_0 = encode_read_bank_prep_frame(6, 0x0018, 3);
    assert_eq!(prep_0, [0x07, 0x18, 0x09, 0x06, 0x18, 0x00, 0x00, 0x03]);

    let prep_3 = encode_read_bank_prep_frame(6, 0x001b, 3);
    assert_eq!(prep_3, [0x07, 0x18, 0x09, 0x06, 0x1b, 0x00, 0x00, 0x03]);

    let arm = encode_read_bank_arm_finalize(6, 0x0018);
    assert_eq!(arm, [0x07, 0x18, 0x00, 0x06, 0x18, 0x00, 0x00, 0x00]);

    let read_0 = encode_read_bank_frame(6, 0x0018, 3);
    assert_eq!(read_0, [0x07, 0x18, 0x05, 0x06, 0x18, 0x00, 0x00, 0x03]);

    let read_3 = encode_read_bank_frame(6, 0x001b, 3);
    assert_eq!(read_3, [0x07, 0x18, 0x05, 0x06, 0x1b, 0x00, 0x00, 0x03]);

    let reset = encode_read_reset_frame();
    assert_eq!(reset, [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn read_sequence_frame_golden_table_banks_0_to_7() {
    // Explicit golden arrays for bank 0..=7 across all three phases at base addr 0x0018, count_minus_1 = 3
    const GOLDEN_PREP: [[u8; 8]; 8] = [
        [0x07, 0x18, 0x09, 0x00, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x09, 0x01, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x09, 0x02, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x09, 0x03, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x09, 0x04, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x09, 0x05, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x09, 0x06, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x09, 0x07, 0x18, 0x00, 0x00, 0x03],
    ];

    const GOLDEN_ARM: [[u8; 8]; 8] = [
        [0x07, 0x18, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00],
        [0x07, 0x18, 0x00, 0x01, 0x18, 0x00, 0x00, 0x00],
        [0x07, 0x18, 0x00, 0x02, 0x18, 0x00, 0x00, 0x00],
        [0x07, 0x18, 0x00, 0x03, 0x18, 0x00, 0x00, 0x00],
        [0x07, 0x18, 0x00, 0x04, 0x18, 0x00, 0x00, 0x00],
        [0x07, 0x18, 0x00, 0x05, 0x18, 0x00, 0x00, 0x00],
        [0x07, 0x18, 0x00, 0x06, 0x18, 0x00, 0x00, 0x00],
        [0x07, 0x18, 0x00, 0x07, 0x18, 0x00, 0x00, 0x00],
    ];

    const GOLDEN_READ: [[u8; 8]; 8] = [
        [0x07, 0x18, 0x05, 0x00, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x05, 0x01, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x05, 0x02, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x05, 0x03, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x05, 0x04, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x05, 0x05, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x05, 0x06, 0x18, 0x00, 0x00, 0x03],
        [0x07, 0x18, 0x05, 0x07, 0x18, 0x00, 0x00, 0x03],
    ];

    for bank in 0..=7u8 {
        let b = bank as usize;
        let prep_frame = ReadSequenceFrame::new(ReadPhase::Prep, bank, 0x0018, 3).encode();
        assert_eq!(prep_frame, GOLDEN_PREP[b]);
        assert_eq!(prep_frame, encode_read_bank_prep_frame(bank, 0x0018, 3));

        let arm_frame = ReadSequenceFrame::new(ReadPhase::ArmFinalize, bank, 0x0018, 0).encode();
        assert_eq!(arm_frame, GOLDEN_ARM[b]);
        assert_eq!(arm_frame, encode_read_bank_arm_finalize(bank, 0x0018));

        let read_frame = ReadSequenceFrame::new(ReadPhase::ReadRequest, bank, 0x0018, 3).encode();
        assert_eq!(read_frame, GOLDEN_READ[b]);
        assert_eq!(read_frame, encode_read_bank_frame(bank, 0x0018, 3));

        // For bank 0, verify exact equivalence with legacy chunk encoders
        if bank == 0 {
            assert_eq!(prep_frame, encode_read_prep_frame(0, 0x0018, 3));
            assert_eq!(arm_frame, encode_read_arm_finalize(0x0018));
            assert_eq!(read_frame, encode_read_sweep_frame(0, 0x0018, 3));
        }
    }
}

#[test]
fn parse_hid_feature_report_size_live_descriptor() {
    use panteon_ms89::codec::parse_hid_feature_report_size;

    // Exact 140-byte report descriptor from PANTEON MS89 Interface 1 (/dev/hidraw8)
    const LIVE_DESC: [u8; 140] = [
        0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x85, 0x03, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7, 0x15,
        0x00, 0x25, 0x01, 0x95, 0x08, 0x75, 0x01, 0x81, 0x02, 0x95, 0x06, 0x75, 0x08, 0x15, 0x00,
        0x26, 0xff, 0x00, 0x05, 0x07, 0x19, 0x00, 0x29, 0xe7, 0x81, 0x00, 0xc0, 0x05, 0x0c, 0x09,
        0x01, 0xa1, 0x01, 0x85, 0x02, 0x19, 0x00, 0x2a, 0x9c, 0x02, 0x15, 0x00, 0x26, 0x9c, 0x02,
        0x95, 0x01, 0x75, 0x10, 0x81, 0x00, 0xc0, 0x06, 0x00, 0xff, 0x09, 0x01, 0xa1, 0x01, 0x85,
        0x06, 0x15, 0x00, 0x26, 0xff, 0x00, 0x09, 0x2f, 0x95, 0x02, 0x75, 0x08, 0x81, 0x02, 0xc0,
        0x06, 0x01, 0xff, 0x09, 0x01, 0xa1, 0x01, 0x85, 0x07, 0x15, 0x00, 0x26, 0xff, 0x00, 0x09,
        0x20, 0x95, 0x07, 0x75, 0x08, 0xb1, 0x02, 0xc0, 0x05, 0x01, 0x09, 0x80, 0xa1, 0x01, 0x85,
        0x08, 0x19, 0x81, 0x29, 0x83, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x03, 0x81, 0x02,
        0x95, 0x05, 0x81, 0x01, 0xc0,
    ];

    // Feature Report 0x07 has Report Count = 7, Report Size = 8 -> 7 bytes data payload
    assert_eq!(parse_hid_feature_report_size(&LIVE_DESC, 0x07), Some(7));

    // Report ID 0x03 is Input, not Feature -> returns None
    assert_eq!(parse_hid_feature_report_size(&LIVE_DESC, 0x03), None);

    // Non-existent Report ID -> None
    assert_eq!(parse_hid_feature_report_size(&LIVE_DESC, 0x99), None);

    // Empty descriptor -> None
    assert_eq!(parse_hid_feature_report_size(&[], 0x07), None);
}

#[test]
fn descriptor_page_decode_golden_live_vector() {
    let page = decode_descriptor_page(&LIVE_DESCRIPTOR_PAGE_BYTES)
        .expect("decode golden live descriptor page");

    assert_eq!(page.sync, (0x5a, 0xa5));
    assert_eq!(page.unlock_state, 0x03);
    assert_eq!(page.pid, 0x1440);
    assert_eq!(page.vid_lo, 0xfa);
    assert_eq!(page.write_echo, 0x30);
    assert_eq!(page.flags, 0x10);
    assert_eq!(
        page.bank_flags,
        [0x21, 0x22, 0x23, 0x24, 0x25, 0x2c, 0x29, 0x2b]
    );
    assert_eq!(
        page.calibration,
        [
            0xff, 0xa1, 0x34, 0xcb, 0xef, 0x1f, 0xf2, 0xfb, 0xfc, 0xc7, 0xe7, 0xbd, 0x77, 0xae,
            0x7f, 0xff
        ]
    );
    assert_eq!(page.raw, LIVE_DESCRIPTOR_PAGE_BYTES);

    // Test formatted table
    let table = page.format_table();
    assert!(table.contains("PID:             0x1440"));
    assert!(table.contains("vid_lo=0xfa"));
    assert!(table.contains("write_echo=0x30"));
    assert!(table.contains("0x5a, 0xa5"));
    assert!(table.contains("Bank 0: 0x21"));
    assert!(table.contains("Bank 6: 0x29"));

    // Test JSON serialization
    let json = page.to_json_string();
    assert!(json.contains("\"pid\": \"0x1440\""));
    assert!(json.contains("\"vid_lo\": \"0xfa\""));
    assert!(json.contains("\"write_echo\": \"0x30\""));
    assert!(json.contains("\"unlock_state\": \"0x03\""));
    assert!(json.contains("\"0x29\""));
}

#[test]
fn descriptor_page_decode_post_write_live_vector() {
    let page = decode_descriptor_page(&POST_WRITE_DESCRIPTOR_PAGE_BYTES)
        .expect("decode post-write live descriptor page");

    assert_eq!(page.sync, (0x03, 0xa5));
    assert_eq!(page.unlock_state, 0x03);
    assert_eq!(page.pid, 0x1440);
    assert_eq!(page.vid_lo, 0xfa);
    assert_eq!(page.write_echo, 0x00);
    assert_eq!(page.flags, 0x10);
    assert_eq!(
        page.bank_flags,
        [0x21, 0x22, 0x23, 0x24, 0x25, 0x2c, 0x29, 0x2b]
    );
    assert_eq!(
        page.calibration,
        [
            0xff, 0xa1, 0x34, 0xcb, 0xef, 0x1f, 0xf2, 0xfb, 0xfc, 0xc7, 0xe7, 0xbd, 0x77, 0xae,
            0x7f, 0xff
        ]
    );
    assert_eq!(page.raw, POST_WRITE_DESCRIPTOR_PAGE_BYTES);

    // Invariant: post-write descriptor does NOT decode as VID 0x30fa
    assert_ne!(u16::from_le_bytes([page.vid_lo, page.write_echo]), 0x30fa);
    assert_eq!(page.vid_lo, 0xfa);
    assert_eq!(page.write_echo, 0x00);

    // Format & JSON tests
    let table = page.format_table();
    assert!(table.contains("write_echo=0x00"));
    let json = page.to_json_string();
    assert!(json.contains("\"write_echo\": \"0x00\""));
}

#[test]
fn descriptor_page_decode_negative_lengths() {
    let err_empty = decode_descriptor_page(&[]).expect_err("empty slice must fail");
    assert_eq!(
        err_empty,
        panteon_ms89::PanteonError::Protocol("descriptor page length must be exactly 32 bytes")
    );

    let err_short = decode_descriptor_page(&[0u8; 31]).expect_err("31-byte slice must fail");
    assert_eq!(
        err_short,
        panteon_ms89::PanteonError::Protocol("descriptor page length must be exactly 32 bytes")
    );

    let err_long = decode_descriptor_page(&[0u8; 33]).expect_err("33-byte slice must fail");
    assert_eq!(
        err_long,
        panteon_ms89::PanteonError::Protocol("descriptor page length must be exactly 32 bytes")
    );
}

#[test]
fn probe_command_encoders_golden() {
    use panteon_ms89::{
        encode_0x15, encode_0x16, encode_0x20, encode_macro_delay,
        encode_packed_mem_mode_0x10, encode_probe_command, Command,
    };

    // Command::MacroDelay (0x0b)
    assert_eq!(Command::from_u8(0x0b).unwrap(), Command::MacroDelay);
    let f_0b = encode_macro_delay(0x0102, 0x0304);
    assert_eq!(f_0b, [0x07, 0x0b, 0x01, 0x02, 0x03, 0x04, 0x00, 0x00]);

    // Command::Unknown15 (0x15)
    let f_15 = encode_0x15([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
    assert_eq!(f_15, [0x07, 0x15, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);

    // Command::Unknown16 (0x16)
    let f_16 = encode_0x16(0x07);
    assert_eq!(f_16, [0x07, 0x16, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00]);

    // Command::Unknown20 (0x20)
    let f_20 = encode_0x20();
    assert_eq!(f_20, [0x07, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // PackedMem Mode 0x10 (Flash persist / flush)
    let f_10 = encode_packed_mem_mode_0x10();
    assert_eq!(f_10, [0x07, 0x18, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // Generic probe encoder
    let f_gen = encode_probe_command(0x13, &[0x00, 0x80, 0x00, 0x40]).unwrap();
    assert_eq!(f_gen, [0x07, 0x13, 0x00, 0x80, 0x00, 0x40, 0x00, 0x00]);

    let f_gen_empty = encode_probe_command(0x20, &[]).unwrap();
    assert_eq!(f_gen_empty, [0x07, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn soft_apply_tail_golden_sequence() {
    use panteon_ms89::codec::{encode_soft_apply_tail, encode_soft_apply_tail_frames};

    let tail = encode_soft_apply_tail();
    assert_eq!(tail.len(), 5);
    // Frame 0: B0/0x007f Write
    assert_eq!(tail[0], [0x07, 0x18, 0x03, 0x00, 0x7f, 0x00, 0x00, 0x00]);
    // Frame 1: B0/0x007f Commit Bank
    assert_eq!(tail[1], [0x07, 0x18, 0x09, 0x00, 0x7f, 0x00, 0x00, 0x00]);
    // Frame 2: B0/0x007f Final Commit
    assert_eq!(tail[2], [0x07, 0x18, 0x00, 0x00, 0x7f, 0x00, 0x00, 0x00]);
    // Frame 3: Mode 0x10 Flash Flush
    assert_eq!(tail[3], [0x07, 0x18, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]);
    // Frame 4: Mode 0x00 Idle
    assert_eq!(tail[4], [0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // Test tail without flash flush (endurance mode)
    let tail_no_flush = encode_soft_apply_tail_frames(false);
    assert_eq!(tail_no_flush.len(), 4);
    assert_eq!(tail_no_flush[0], tail[0]);
    assert_eq!(tail_no_flush[1], tail[1]);
    assert_eq!(tail_no_flush[2], tail[2]);
    assert_eq!(tail_no_flush[3], tail[4]);
}


#[test]
fn probe_command_encoder_negative() {
    use panteon_ms89::{encode_probe_command, Command};

    // Unknown command
    assert!(Command::from_u8(0xff).is_err());

    // Too many arguments (> 6 bytes)
    let err = encode_probe_command(0x0b, &[1, 2, 3, 4, 5, 6, 7]).unwrap_err();
    match err {
        panteon_ms89::PanteonError::ProtocolDetailed { context, detail } => {
            assert_eq!(context, "encode_probe_command");
            assert!(detail.contains("too many args (7)"));
        }
        _ => panic!("Expected ProtocolDetailed error, got: {:?}", err),
    }
}

#[test]
fn fire_delay_golden_8ms_and_300ms() {
    use panteon_ms89::service::plan_fire_delay;

    // 1. 8 ms (observed in fire_sp_to-8ms.pcap, frames 25 and 26)
    let (p8_low, p8_high) = plan_fire_delay(8).expect("plan_fire_delay(8) should succeed");
    assert_eq!(p8_low.bank, 0x02);
    assert_eq!(p8_low.addr, 0x0018);
    assert_eq!(p8_low.data, 0x0708);
    assert_eq!(p8_low.data_frame, [0x07, 0x18, 0x03, 0x02, 0x18, 0x00, 0x08, 0x07]);

    assert_eq!(p8_high.bank, 0x03);
    assert_eq!(p8_high.addr, 0x0018);
    assert_eq!(p8_high.data, 0x0700);
    assert_eq!(p8_high.data_frame, [0x07, 0x18, 0x03, 0x03, 0x18, 0x00, 0x00, 0x07]);

    // 2. 300 ms (observed in fire_sp_to-300.pcap, frames 25 and 26: 300 = 0x012c -> low=0x2c, high=0x01)
    let (p300_low, p300_high) = plan_fire_delay(300).expect("plan_fire_delay(300) should succeed");
    assert_eq!(p300_low.bank, 0x02);
    assert_eq!(p300_low.addr, 0x0018);
    assert_eq!(p300_low.data, 0x072c);
    assert_eq!(p300_low.data_frame, [0x07, 0x18, 0x03, 0x02, 0x18, 0x00, 0x2c, 0x07]);

    assert_eq!(p300_high.bank, 0x03);
    assert_eq!(p300_high.addr, 0x0018);
    assert_eq!(p300_high.data, 0x0701);
    assert_eq!(p300_high.data_frame, [0x07, 0x18, 0x03, 0x03, 0x18, 0x00, 0x01, 0x07]);

    // 3. 50 ms default (observed in reset frames 25 and 26: 50 = 0x0032 -> low=0x32, high=0x00)
    let (p50_low, p50_high) = plan_fire_delay(50).expect("plan_fire_delay(50) should succeed");
    assert_eq!(p50_low.data_frame, [0x07, 0x18, 0x03, 0x02, 0x18, 0x00, 0x32, 0x07]);
    assert_eq!(p50_high.data_frame, [0x07, 0x18, 0x03, 0x03, 0x18, 0x00, 0x00, 0x07]);
}

#[test]
fn fire_delay_bounds_validation() {
    use panteon_ms89::service::plan_fire_delay;

    assert!(plan_fire_delay(0).is_err());
    assert!(plan_fire_delay(7).is_err());
    assert!(plan_fire_delay(8).is_ok());
    assert!(plan_fire_delay(300).is_ok());
    assert!(plan_fire_delay(301).is_err());
    assert!(plan_fire_delay(1000).is_err());
}

#[test]
fn brightness_bank7_levels_and_bounds() {
    use panteon_ms89::codec::{brightness_to_bank7, bank7_to_brightness, encode_color_block_38};

    // 25% -> Level 2 -> 0x0760
    assert_eq!(brightness_to_bank7(2).unwrap(), 0x0760);
    assert_eq!(bank7_to_brightness(0x0760).unwrap(), 2);
    let p25 = encode_color_block_38(2).expect("encode 25% brightness");
    assert_eq!(p25[7].bank, 7);
    assert_eq!(p25[7].addr, 0x0038);
    assert_eq!(p25[7].data, 0x0760);

    // 50% -> Level 4 -> 0x0740
    assert_eq!(brightness_to_bank7(4).unwrap(), 0x0740);
    assert_eq!(bank7_to_brightness(0x0740).unwrap(), 4);
    let p50 = encode_color_block_38(4).expect("encode 50% brightness");
    assert_eq!(p50[7].data, 0x0740);

    // 100% -> Level 8 -> 0x0700
    assert_eq!(brightness_to_bank7(8).unwrap(), 0x0700);
    assert_eq!(bank7_to_brightness(0x0700).unwrap(), 8);
    let p100 = encode_color_block_38(8).expect("encode 100% brightness");
    assert_eq!(p100[7].data, 0x0700);

    // Bounds validation
    assert!(brightness_to_bank7(9).is_err());
    assert!(encode_color_block_38(9).is_err());
}
