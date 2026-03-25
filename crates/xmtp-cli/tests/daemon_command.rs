use assert_cmd::Command;
use predicates::str::contains;
use tempfile::tempdir;

#[test]
fn daemon_start_makes_status_available_and_stop_shuts_it_down() {
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
        .args(["daemon", "start"])
        .assert()
        .success()
        .stdout(contains("daemon_started"));

    Command::cargo_bin("xmtp-cli")
        .expect("binary")
        .args(["--data-dir"])
        .arg(&data_dir)
        .args(["daemon", "status"])
        .assert()
        .success()
        .stdout(contains("daemon_state"))
        .stdout(contains("connection_state"));

    Command::cargo_bin("xmtp-cli")
        .expect("binary")
        .args(["--data-dir"])
        .arg(&data_dir)
        .args(["daemon", "stop"])
        .assert()
        .success()
        .stdout(contains("daemon_stopped"));

    Command::cargo_bin("xmtp-cli")
        .expect("binary")
        .args(["--data-dir"])
        .arg(&data_dir)
        .args(["daemon", "status"])
        .assert()
        .failure();
}

#[test]
fn daemon_restart_keeps_status_available() {
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
        .args(["daemon", "restart"])
        .assert()
        .success()
        .stdout(contains("daemon_restarted"));

    Command::cargo_bin("xmtp-cli")
        .expect("binary")
        .args(["--data-dir"])
        .arg(&data_dir)
        .args(["daemon", "status"])
        .assert()
        .success()
        .stdout(contains("daemon_state"))
        .stdout(contains("connection_state"));
}
