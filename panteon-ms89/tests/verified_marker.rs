use std::fs;
use panteon_ms89::device::check_persisted_verification_with_marker_file;
use panteon_ms89::store::{
    load_verified_store, parse_verified_store, remove_verified_marker_file,
    save_verified_store_atomic, verified_store_to_toml, VerifiedStore,
};

#[test]
fn test_verified_marker_roundtrip() {
    let mut store = VerifiedStore::default();
    store.insert_or_update(0x1e01, vec![0x0976, 0x0978], 1762970000);
    store.insert_or_update(0x1440, vec![0x0976], 1762960000);

    let toml_str = verified_store_to_toml(&store);
    assert!(toml_str.contains("[[verified]]"));
    assert!(toml_str.contains("pid = 0x1e01"));
    assert!(toml_str.contains("signatures = [\"0x0976\", \"0x0978\"]"));
    assert!(toml_str.contains("verified_at = 1762970000"));
    assert!(toml_str.contains("pid = 0x1440"));

    let parsed = parse_verified_store(&toml_str).expect("parse_verified_store should succeed");
    assert_eq!(parsed.entries.len(), 2);

    let entry_1e01 = parsed.find(0x1e01).expect("0x1e01 must be present");
    assert_eq!(entry_1e01.pid, 0x1e01);
    assert_eq!(entry_1e01.signatures, vec![0x0976, 0x0978]);
    assert_eq!(entry_1e01.verified_at, 1762970000);

    let entry_1440 = parsed.find(0x1440).expect("0x1440 must be present");
    assert_eq!(entry_1440.pid, 0x1440);
    assert_eq!(entry_1440.signatures, vec![0x0976]);
    assert_eq!(entry_1440.verified_at, 1762960000);
}

#[test]
fn test_verified_marker_mismatch_case() {
    let temp_dir = std::env::temp_dir().join(format!("test_mismatch_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).unwrap();
    let marker_file = temp_dir.join("verified.toml");

    let mut store = VerifiedStore::default();
    // Marker contains only 0x0976
    store.insert_or_update(0x1e01, vec![0x0976], 1762970000);
    save_verified_store_atomic(&marker_file, &store).unwrap();

    // Actual signature 0x0976 matches marker -> returns true
    assert!(check_persisted_verification_with_marker_file(&marker_file, 0x1e01, 0x0976));

    // Actual signature 0x0978 NOT in marker -> returns false (protection against firmware change)
    assert!(!check_persisted_verification_with_marker_file(&marker_file, 0x1e01, 0x0978));

    // Different PID 0x1440 not in marker -> returns false
    assert!(!check_persisted_verification_with_marker_file(&marker_file, 0x1440, 0x0976));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_verified_marker_missing_case() {
    let non_existent = std::env::temp_dir().join(format!("non_existent_{}.toml", std::process::id()));
    let _ = fs::remove_file(&non_existent);

    // Missing file should return default empty store without error
    let store = load_verified_store(&non_existent).unwrap();
    assert!(store.entries.is_empty());
    assert!(store.find(0x1e01).is_none());

    // check_persisted_verification should return false
    assert!(!check_persisted_verification_with_marker_file(&non_existent, 0x1e01, 0x0978));
}

#[test]
fn test_forget_verification_removes_entry_and_file() {
    let temp_dir = std::env::temp_dir().join(format!("test_forget_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).unwrap();
    let marker_file = temp_dir.join("verified.toml");

    let mut store = VerifiedStore::default();
    store.insert_or_update(0x1e01, vec![0x0976, 0x0978], 1762970000);
    store.insert_or_update(0x1440, vec![0x0976], 1762960000);
    save_verified_store_atomic(&marker_file, &store).unwrap();

    assert!(marker_file.exists());

    // Remove single entry
    let mut loaded = load_verified_store(&marker_file).unwrap();
    assert!(loaded.remove(0x1e01));
    assert_eq!(loaded.entries.len(), 1);
    assert!(loaded.find(0x1e01).is_none());
    assert!(loaded.find(0x1440).is_some());
    save_verified_store_atomic(&marker_file, &loaded).unwrap();

    // Remove file completely
    assert!(remove_verified_marker_file(&marker_file).unwrap());
    assert!(!marker_file.exists());

    let _ = fs::remove_dir_all(&temp_dir);
}
