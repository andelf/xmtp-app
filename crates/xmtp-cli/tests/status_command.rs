use assert_cmd::Command;
use predicates::str::contains;
use tempfile::tempdir;

#[test]
fn doctor_command_prints_stopped_state_after_init() {
    let temp = tempdir().expect("tempdir");
    let data_dir = temp.path().join("data");

    Command::cargo_bin("xmtp-cli")
        .expect("binary")
        .args(["--data-dir"])
        .arg(&data_dir)
        .arg("init")
        .assert()
        .success();

    Command::cargo_bin("xmtp-cli")
        .expect("binary")
        .args(["--data-dir"])
        .arg(&data_dir)
        .arg("doctor")
        .assert()
        .success()
        .stdout(contains("network"))
        .stdout(contains("daemon_reachable"))
        .stdout(contains("stopped"))
        .stdout(contains("disconnected"));
}
