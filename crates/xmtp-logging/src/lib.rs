use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;

pub fn logs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

pub fn daemon_stdout_log_path(data_dir: &Path) -> PathBuf {
    logs_dir(data_dir).join("daemon.stdout.log")
}

pub fn daemon_stderr_log_path(data_dir: &Path) -> PathBuf {
    logs_dir(data_dir).join("daemon.stderr.log")
}

pub fn daemon_events_log_path(data_dir: &Path) -> PathBuf {
    logs_dir(data_dir).join("daemon.events.log")
}

pub fn ensure_logs_dir(data_dir: &Path) -> anyhow::Result<PathBuf> {
    let dir = logs_dir(data_dir);
    fs::create_dir_all(&dir).context("create logs dir")?;
    Ok(dir)
}

pub fn append_daemon_event(data_dir: &Path, level: &str, message: &str) -> anyhow::Result<()> {
    ensure_logs_dir(data_dir)?;
    let path = daemon_events_log_path(data_dir);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open daemon events log at {}", path.display()))?;
    let timestamp = chrono::Utc::now().to_rfc3339();
    writeln!(file, "{timestamp}\t{level}\t{message}")
        .with_context(|| format!("write daemon events log at {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        append_daemon_event, daemon_events_log_path, daemon_stderr_log_path,
        daemon_stdout_log_path, logs_dir,
    };

    #[test]
    fn append_daemon_event_creates_log_file() {
        let temp = tempfile::tempdir().expect("temp dir");
        append_daemon_event(temp.path(), "info", "daemon started").expect("append event");

        let content =
            std::fs::read_to_string(daemon_events_log_path(temp.path())).expect("read event log");
        assert!(content.contains("\tinfo\tdaemon started"));
    }

    #[test]
    fn log_paths_live_under_logs_dir() {
        let temp = tempfile::tempdir().expect("temp dir");
        let dir = logs_dir(temp.path());

        assert_eq!(
            daemon_stdout_log_path(temp.path()),
            dir.join("daemon.stdout.log")
        );
        assert_eq!(
            daemon_stderr_log_path(temp.path()),
            dir.join("daemon.stderr.log")
        );
        assert_eq!(
            daemon_events_log_path(temp.path()),
            dir.join("daemon.events.log")
        );
    }
}
