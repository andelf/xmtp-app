use tempfile::tempdir;
use xmtp_config::{AppConfig, load_config, save_config};

#[test]
fn config_roundtrips_to_json_file() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("config.json");
    let config = AppConfig {
        schema_version: 1,
        profile: "default".to_owned(),
        xmtp_env: "dev".to_owned(),
        data_dir: temp.path().display().to_string(),
        ipc_socket_path: temp.path().join("daemon.sock").display().to_string(),
        log_level: "info".to_owned(),
        api_url: Some("https://grpc.testnet.xmtp.network:443".to_owned()),
    };

    save_config(&path, &config).expect("save config");
    let loaded = load_config(&path).expect("load config");

    assert_eq!(loaded.schema_version, 1);
    assert_eq!(loaded.profile, "default");
    assert_eq!(loaded.xmtp_env, "dev");
    assert_eq!(loaded.log_level, "info");
    assert_eq!(
        loaded.api_url.as_deref(),
        Some("https://grpc.testnet.xmtp.network:443")
    );
}
