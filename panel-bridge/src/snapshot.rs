// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Dhanvin Raj Velagapudi
//! The status document and how it is written.
//!
//! Kept free of host calls so the shape can be tested natively: the panel
//! reads this file with no knowledge of Pumpkin, so the JSON layout is a
//! contract, and `SCHEMA` is how a reader notices when it changes.

use serde::Serialize;
use std::path::Path;

/// Bumped whenever a field is removed or changes meaning. Adding a field is
/// not a breaking change and does not bump it.
pub const SCHEMA: u32 = 1;

/// Name of the file inside the plugin's data folder.
pub const FILE_NAME: &str = "status.json";

/// What the server is doing, as far as this plugin can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Normal operation; the file is being refreshed.
    Running,
    /// The plugin is unloading, so the server is shutting down. Written once,
    /// last, so a reader can tell a clean stop from a crash: after a crash the
    /// state stays `running` and `updated_at` stops advancing.
    Stopping,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerEntry {
    pub name: String,
    pub uuid: String,
    pub world: String,
    pub gamemode: &'static str,
    /// Round-trip latency in milliseconds, as the server measures it.
    pub ping_ms: u32,
    pub health: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerEntry {
    pub tps: f64,
    pub mspt: f64,
    pub players_online: u32,
    pub max_players: u32,
    pub motd: String,
    pub online_mode: bool,
    pub hardcore: bool,
    pub difficulty: &'static str,
    pub whitelist: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub schema: u32,
    pub state: State,
    /// Seconds since the Unix epoch when this snapshot was taken.
    pub updated_at: i64,
    pub server: ServerEntry,
    pub players: Vec<PlayerEntry>,
}

/// Seconds since the Unix epoch.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Replaces non-finite floats, which JSON cannot represent.
///
/// `serde_json` refuses to serialise `NaN` and infinity, and a TPS reading
/// can be one of those in the first moments after startup. Losing the whole
/// snapshot over that would be a poor trade, so it becomes zero.
pub fn finite(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

impl Snapshot {
    /// Writes the snapshot into `folder`, replacing the previous one.
    ///
    /// Written beside the target then renamed, so a reader polling the file
    /// never sees half a document.
    pub fn write(&self, folder: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let path = folder.join(FILE_NAME);
        let temp = folder.join(format!("{FILE_NAME}.tmp"));
        std::fs::write(&temp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&temp, &path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Snapshot {
        Snapshot {
            schema: SCHEMA,
            state: State::Running,
            updated_at: 1_700_000_000,
            server: ServerEntry {
                tps: 20.0,
                mspt: 1.5,
                players_online: 1,
                max_players: 20,
                motd: "A Pumpkin Server".into(),
                online_mode: true,
                hardcore: false,
                difficulty: "normal",
                whitelist: false,
            },
            players: vec![PlayerEntry {
                name: "Steve".into(),
                uuid: "00000000-0000-0000-0000-000000000001".into(),
                world: "minecraft:overworld".into(),
                gamemode: "survival",
                ping_ms: 23,
                health: 20.0,
            }],
        }
    }

    #[test]
    fn json_layout_is_the_documented_contract() {
        let value = serde_json::to_value(sample()).unwrap();
        assert_eq!(value["schema"], 1);
        assert_eq!(value["state"], "running");
        assert_eq!(value["server"]["players_online"], 1);
        assert_eq!(value["players"][0]["name"], "Steve");
        assert_eq!(value["players"][0]["ping_ms"], 23);
    }

    #[test]
    fn stopping_state_serialises_lowercase() {
        let mut snapshot = sample();
        snapshot.state = State::Stopping;
        let value = serde_json::to_value(snapshot).unwrap();
        assert_eq!(value["state"], "stopping");
    }

    #[test]
    fn non_finite_numbers_become_zero() {
        assert_eq!(finite(f64::NAN), 0.0);
        assert_eq!(finite(f64::INFINITY), 0.0);
        assert_eq!(finite(19.9), 19.9);
    }

    #[test]
    fn write_replaces_the_file_without_leaving_a_temp_behind() {
        let folder = std::env::temp_dir().join(format!("panel-bridge-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();

        sample().write(&folder).unwrap();
        let mut second = sample();
        second.updated_at += 5;
        second.write(&folder).unwrap();

        let text = std::fs::read_to_string(folder.join(FILE_NAME)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["updated_at"], 1_700_000_005_i64);
        assert!(!folder.join("status.json.tmp").exists());

        std::fs::remove_dir_all(&folder).unwrap();
    }
}
