use std::fs;

use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn init_command_creates_config_and_state_files() {
    let temp = tempdir().expect("tempdir");
    let data_dir = temp.path().join("data");

    Command::cargo_bin("xmtp-cli")
        .expect("binary")
        .args(["--data-dir"])
        .arg(&data_dir)
        .arg("init")
        .assert()
        .success();

    assert!(data_dir.join("config.json").exists());
    assert!(data_dir.join("state.json").exists());

    let config = fs::read_to_string(data_dir.join("config.json")).expect("read config");
    let state = fs::read_to_string(data_dir.join("state.json")).expect("read state");

    assert!(config.contains("\"profile\": \"default\""));
    assert!(state.contains("\"daemon_state\": \"stopped\""));
}

#[test]
fn init_command_defaults_to_local_data_directory() {
    let temp = tempdir().expect("tempdir");
    let data_dir = temp.path().join("data");

    Command::cargo_bin("xmtp-cli")
        .expect("binary")
        .current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    assert!(data_dir.join("config.json").exists());
    assert!(data_dir.join("state.json").exists());
}
