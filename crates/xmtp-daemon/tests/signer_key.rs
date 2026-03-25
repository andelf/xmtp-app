use tempfile::tempdir;
use xmtp_daemon::load_or_create_signer_key_hex;

#[test]
fn load_or_create_signer_key_reuses_existing_private_key() {
    let temp = tempdir().expect("tempdir");
    let data_dir = temp.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("create data dir");

    let first = load_or_create_signer_key_hex(&data_dir).expect("first key");
    let second = load_or_create_signer_key_hex(&data_dir).expect("second key");

    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
}
