use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn status_command_prints_stopped_state_after_init() {
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
        .arg("status")
        .assert()
        .success()
        .stdout(predicates::str::contains("stopped"))
        .stdout(predicates::str::contains("disconnected"));
}
