use std::path::Path;

use tempfile::tempdir;
use xmtp_config::{AppConfig, save_config};
use xmtp_core::{ConnectionState, DaemonState, StateSnapshot, SyncPhase, SyncState};
use xmtp_daemon::{RuntimeInfo, XmtpRuntimeAdapter, login_with_adapter};
use xmtp_store::{load_state, save_state};

struct FakeAdapter;

impl XmtpRuntimeAdapter for FakeAdapter {
    fn connect(&self, _env: &str, _data_dir: &Path) -> anyhow::Result<RuntimeInfo> {
        Ok(RuntimeInfo {
            inbox_id: "inbox-123".to_owned(),
            installation_id: "install-123".to_owned(),
        })
    }
}

#[test]
fn login_updates_state_with_connected_runtime_info() {
    let temp = tempdir().expect("tempdir");
    let data_dir = temp.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("create data dir");

    let state = StateSnapshot {
        schema_version: 1,
        daemon_state: DaemonState::Stopped,
        started_at_unix_ms: None,
        current_profile: Some("default".to_owned()),
        inbox_id: None,
        installation_id: None,
        connection_state: ConnectionState::Disconnected,
        sync_state: SyncState {
            phase: SyncPhase::Idle,
            last_cursor: None,
            last_successful_sync_unix_ms: None,
            pending_actions: 0,
        },
        recent_error: None,
    };
    let config = AppConfig::for_data_dir(&data_dir);
    save_config(&data_dir.join("config.json"), &config).expect("save config");
    save_state(&data_dir.join("state.json"), &state).expect("save state");

    login_with_adapter(&FakeAdapter, "dev", &data_dir).expect("login");

    let updated = load_state(&data_dir.join("state.json")).expect("load state");
    assert!(matches!(updated.daemon_state, DaemonState::Running));
    assert!(matches!(updated.connection_state, ConnectionState::Connected));
    assert_eq!(updated.inbox_id.as_deref(), Some("inbox-123"));
    assert_eq!(updated.installation_id.as_deref(), Some("install-123"));
}
