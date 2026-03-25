use tempfile::tempdir;
use xmtp_core::{ConnectionState, DaemonState, StateSnapshot, SyncPhase, SyncState};
use xmtp_store::{load_state, save_state};

#[test]
fn state_snapshot_roundtrips_to_json_file() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("state.json");
    let snapshot = StateSnapshot {
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

    save_state(&path, &snapshot).expect("save state");
    let loaded = load_state(&path).expect("load state");

    assert_eq!(loaded.schema_version, 1);
    assert!(matches!(loaded.daemon_state, DaemonState::Stopped));
    assert!(matches!(loaded.connection_state, ConnectionState::Disconnected));
    assert!(matches!(loaded.sync_state.phase, SyncPhase::Idle));
}
