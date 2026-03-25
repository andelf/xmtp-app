use std::fs;
use std::path::Path;

use anyhow::Context;
use xmtp_core::StateSnapshot;

pub fn save_state(path: &Path, snapshot: &StateSnapshot) -> anyhow::Result<()> {
    let parent = path.parent().context("state path has no parent")?;
    fs::create_dir_all(parent).context("create state directory")?;
    let json = serde_json::to_string_pretty(snapshot).context("serialize state")?;
    fs::write(path, json).context("write state")?;
    Ok(())
}

pub fn load_state(path: &Path) -> anyhow::Result<StateSnapshot> {
    let json = fs::read_to_string(path).context("read state")?;
    let snapshot = serde_json::from_str(&json).context("deserialize state")?;
    Ok(snapshot)
}
