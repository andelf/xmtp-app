use assert_cmd::Command;
use predicates::str::contains;
use tempfile::tempdir;

#[test]
fn daemon_status_fails_with_meaningful_error_when_daemon_is_not_running() {
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
        .args(["daemon", "status"])
        .assert()
        .failure()
        .stderr(contains("read daemon addr file"));
}
