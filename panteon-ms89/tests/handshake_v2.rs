use panteon_ms89::codec::DeviceGetReportResponse;
use panteon_ms89::model::DeviceState;
use panteon_ms89::PanteonError;

#[test]
fn test_golden_state_0x00() {
    // 07 00 78 09 <rev> 00 23 <flags>
    let raw = [0x07, 0x00, 0x78, 0x09, 0x3c, 0x00, 0x23, 0x7a];
    let resp = DeviceGetReportResponse::decode_with_expected_signature(&raw, 0x0978)
        .expect("state 0x00 with 0x0978 must decode cleanly");

    assert_eq!(resp.report_id, 0x07);
    assert_eq!(resp.command, 0x00);
    assert_eq!(resp.state_byte, 0x00);
    assert_eq!(resp.signature, 0x0978);
    assert_eq!(resp.revision, 0x3c);
    assert_eq!(resp.status, 0x23);
    assert_eq!(resp.flags, 0x7a);
}

#[test]
fn test_golden_state_0xdf() {
    // 07 df 76 09 <rev> 00 23 <flags>
    let raw = [0x07, 0xdf, 0x76, 0x09, 0x4e, 0x00, 0x23, 0x5a];
    let resp = DeviceGetReportResponse::decode_with_expected_signature(&raw, 0x0976)
        .expect("state 0xdf with 0x0976 must decode cleanly");

    assert_eq!(resp.report_id, 0x07);
    assert_eq!(resp.command, 0xdf);
    assert_eq!(resp.state_byte, 0xdf);
    assert_eq!(resp.signature, 0x0976);
    assert_eq!(resp.revision, 0x4e);
    assert_eq!(resp.status, 0x23);
    assert_eq!(resp.flags, 0x5a);
}

#[test]
fn test_cross_format_rejected_only_on_signature_mismatch() {
    let buf_a_78 = [0x07, 0x00, 0x78, 0x09, 0x51, 0x00, 0x23, 0x7a];
    let buf_b_76 = [0x07, 0xdf, 0x76, 0x09, 0x52, 0x00, 0x23, 0x7b];

    // State 0xdf is accepted when its signature 0x0976 matches expected
    assert!(DeviceGetReportResponse::decode_with_expected_signature(&buf_b_76, 0x0976).is_ok());

    // State 0x00 is accepted when its signature 0x0978 matches expected
    assert!(DeviceGetReportResponse::decode_with_expected_signature(&buf_a_78, 0x0978).is_ok());

    // Rejected ONLY when signature does not match expected
    let err_b_mismatch = DeviceGetReportResponse::decode_with_expected_signature(&buf_b_76, 0x0978).unwrap_err();
    match err_b_mismatch {
        PanteonError::ProtocolDetailed { detail, .. } => {
            assert!(detail.contains("0x0976"));
            assert!(detail.contains("0x0978"));
            assert!(detail.contains("raw=[07 df 76 09 52 00 23 7b]"));
        }
        other => panic!("Expected ProtocolDetailed, got: {:?}", other),
    }

    let err_a_mismatch = DeviceGetReportResponse::decode_with_expected_signature(&buf_a_78, 0x0976).unwrap_err();
    match err_a_mismatch {
        PanteonError::ProtocolDetailed { detail, .. } => {
            assert!(detail.contains("0x0978"));
            assert!(detail.contains("0x0976"));
            assert!(detail.contains("raw=[07 00 78 09 51 00 23 7a]"));
        }
        other => panic!("Expected ProtocolDetailed, got: {:?}", other),
    }

    // Both are accepted when accepted_signatures = [0x0976, 0x0978]
    let accepted = [0x0976, 0x0978];
    assert!(DeviceGetReportResponse::decode_with_expected_signatures(&buf_a_78, &accepted).is_ok());
    assert!(DeviceGetReportResponse::decode_with_expected_signatures(&buf_b_76, &accepted).is_ok());
}

#[test]
fn test_raw_bytes_in_error_diagnostics() {
    // Signature mismatch
    let bad_sig = [0x07, 0xee, 0x34, 0x12, 0x3c, 0x00, 0x23, 0x00];
    let err = DeviceGetReportResponse::decode_slice(&bad_sig).unwrap_err();
    match err {
        PanteonError::ProtocolDetailed { detail, .. } => {
            assert!(detail.contains("0x1234"));
            assert!(detail.contains("raw=[07 ee 34 12 3c 00 23 00]"));
        }
        other => panic!("Expected ProtocolDetailed, got: {:?}", other),
    }

    // Truncated buffer
    let truncated = [0x07, 0x00, 0x78];
    let err_trunc = DeviceGetReportResponse::decode_slice(&truncated).unwrap_err();
    match err_trunc {
        PanteonError::ProtocolDetailed { detail, .. } => {
            assert!(detail.contains("Truncated"));
            assert!(detail.contains("raw=[07 00 78]"));
        }
        other => panic!("Expected ProtocolDetailed, got: {:?}", other),
    }

    // Invalid report ID
    let bad_id = [0x08, 0x00, 0x76, 0x09, 0x3c, 0x00, 0x23, 0x7a];
    let err_id = DeviceGetReportResponse::decode_slice(&bad_id).unwrap_err();
    match err_id {
        PanteonError::ProtocolDetailed { detail, .. } => {
            assert!(detail.contains("Invalid report ID"));
            assert!(detail.contains("raw=[08 00 76 09 3c 00 23 7a]"));
        }
        other => panic!("Expected ProtocolDetailed, got: {:?}", other),
    }
}

#[test]
fn test_dynamic_revision_byte_does_not_affect_verification() {
    // Revisions seen across live captures: 0x3c, 0x51, 0x52, 0x54, 0x4e, 0x1e
    let revisions = [0x3c, 0x51, 0x52, 0x54, 0x4e, 0x1e];
    for rev in revisions {
        let raw_a = [0x07, 0x00, 0x78, 0x09, rev, 0x00, 0x23, 0x7a];
        let resp_a = DeviceGetReportResponse::decode_slice(&raw_a).unwrap();
        assert_eq!(resp_a.revision, rev);
        assert_eq!(resp_a.state_byte, 0x00);

        let raw_b = [0x07, 0xdf, 0x76, 0x09, rev, 0x00, 0x23, 0x5a];
        let resp_b = DeviceGetReportResponse::decode_slice(&raw_b).unwrap();
        assert_eq!(resp_b.revision, rev);
        assert_eq!(resp_b.state_byte, 0xdf);

        // Also test DeviceState::from_feature_report with dynamic revision
        let state_a = DeviceState::from_feature_report(&raw_a).unwrap();
        assert_eq!(state_a.handshake.unwrap().revision, rev);

        let state_b = DeviceState::from_feature_report(&raw_b).unwrap();
        assert_eq!(state_b.handshake.unwrap().revision, rev);
    }
}
