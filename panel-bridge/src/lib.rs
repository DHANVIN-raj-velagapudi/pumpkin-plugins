// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Dhanvin Raj Velagapudi
//! panel-bridge — publishes live server state to a JSON file.
//!
//! A control panel sits outside the server process. It can start and stop
//! Pumpkin and read its console, but it has no WebAssembly runtime and no
//! channel into the running server, so it cannot ask "what is the TPS?" or
//! "who is online, and where?" without scraping text or opening a query
//! socket. A plugin runs *inside* the server and can answer both, and the one
//! thing the panel can always do is read a file.
//!
//! So this plugin writes `plugins/data/panel-bridge/status.json` about once a
//! second. The layout is documented by [`snapshot`]; the panel needs no
//! knowledge of Pumpkin beyond that path.
//!
//! The only permission requested is write access to the plugin's own data
//! folder. Nothing is sent over the network.

mod snapshot;

use pumpkin_plugin_api::common::GameMode;
use pumpkin_plugin_api::scheduler::SchedulerExt;
use pumpkin_plugin_api::server::Difficulty;
use pumpkin_plugin_api::{
    permissions, register_plugin, uuid, Context, Plugin, PluginMetadata, Result, Server,
};
use snapshot::{finite, now, PlayerEntry, ServerEntry, Snapshot, State, SCHEMA};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// How often the file is refreshed. Twenty ticks is one second at a healthy
/// 20 TPS; on a struggling server the task slows with it, which is the right
/// behaviour for a status file because a stale `updated_at` *is* the signal.
const REFRESH_TICKS: u64 = 20;

/// The tick rate the server aims for.
///
/// Pumpkin derives TPS from how long ticks actually take, with no ceiling, so
/// an idle server that finishes a tick in 0.1 ms reports about 10,000. Vanilla
/// reports at most its tick rate, and a panel graph wants that: "20 means
/// healthy, less means struggling". Clamping here keeps the number meaningful.
const TARGET_TPS: f64 = 20.0;

static DATA_FOLDER: OnceLock<PathBuf> = OnceLock::new();

fn gamemode_name(mode: GameMode) -> &'static str {
    match mode {
        GameMode::Survival => "survival",
        GameMode::Creative => "creative",
        GameMode::Adventure => "adventure",
        GameMode::Spectator => "spectator",
    }
}

fn difficulty_name(difficulty: Difficulty) -> &'static str {
    match difficulty {
        Difficulty::Peaceful => "peaceful",
        Difficulty::Easy => "easy",
        Difficulty::Normal => "normal",
        Difficulty::Hard => "hard",
    }
}

/// Reads everything from the live server in one pass.
fn capture(server: &Server, state: State) -> Snapshot {
    let players = server
        .get_all_players()
        .iter()
        .map(|player| PlayerEntry {
            name: player.get_name(),
            uuid: uuid::to_string(player.get_id()),
            world: player.get_world().get_name(),
            gamemode: gamemode_name(player.get_gamemode()),
            ping_ms: player.get_ping(),
            health: player.get_health(),
        })
        .collect::<Vec<_>>();

    Snapshot {
        schema: SCHEMA,
        state,
        updated_at: now(),
        server: ServerEntry {
            tps: finite(server.get_tps()).min(TARGET_TPS),
            mspt: finite(server.get_mspt()),
            players_online: players.len() as u32,
            max_players: server.get_max_players(),
            motd: server.get_motd(),
            online_mode: server.is_online_mode(),
            hardcore: server.is_hardcore(),
            difficulty: difficulty_name(server.get_difficulty()),
            whitelist: server.has_whitelist(),
        },
        players,
    }
}

/// Captures and writes, logging rather than failing: a status file that could
/// not be written must never take the server down with it.
fn publish(server: &Server, folder: &Path, state: State) {
    if let Err(problem) = capture(server, state).write(folder) {
        tracing::warn!("could not write {}: {problem}", snapshot::FILE_NAME);
    }
}

struct PanelBridge;

impl Plugin for PanelBridge {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "panel-bridge".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Dhanvin".into()],
            description: "Publishes live server status to a JSON file for external tools.".into(),
            dependencies: vec![],
            // Its own data folder, and only to write the status file.
            permissions: vec![permissions::FS_WRITE_DATA.into()],
        }
    }

    fn on_load(&self, context: Context) -> Result<()> {
        let folder = PathBuf::from(context.get_data_folder());
        std::fs::create_dir_all(&folder).map_err(|e| format!("no data folder: {e}"))?;
        let _ = DATA_FOLDER.set(folder.clone());

        // One snapshot straight away so the file exists before the first tick.
        publish(&context.get_server(), &folder, State::Running);

        context.schedule_repeating_task(REFRESH_TICKS, REFRESH_TICKS, move |server| {
            publish(&server, &folder, State::Running);
        });

        tracing::info!("publishing status to {}", snapshot::FILE_NAME);
        Ok(())
    }

    fn on_unload(&self, context: Context) -> Result<()> {
        if let Some(folder) = DATA_FOLDER.get() {
            publish(&context.get_server(), folder, State::Stopping);
        }
        Ok(())
    }
}

register_plugin!(PanelBridge);
