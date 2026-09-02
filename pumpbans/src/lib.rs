//! PumpBans — temporary bans and warnings for Pumpkin.
//!
//! Pumpkin can already ban a player for a fixed period: `BanPlayerOptions`
//! carries a duration, and the server enforces expiry and persists the list
//! itself. What it has no *command* for is setting one — vanilla `/ban` takes a
//! reason and nothing else, so a moderator wanting "two hours" has to ban
//! permanently and remember to come back.
//!
//! So this plugin adds the missing command rather than a second ban system, and
//! adds warnings, which the server genuinely has no concept of.

mod store;

use pumpkin_plugin_api::command::{
    Arg, ArgumentType, Command, CommandError, CommandNode, CommandSender, ConsumedArgs, StringType,
};
use pumpkin_plugin_api::player::BanPlayerOptions;
use pumpkin_plugin_api::text::TextComponent;
use pumpkin_plugin_api::{
    permissions, register_plugin, Context, Plugin, PluginMetadata, Result, Server,
};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use store::{format_duration, now, parse_duration, Store, Warning};

/// Learned from the context at load time.
static DATA_FOLDER: OnceLock<PathBuf> = OnceLock::new();
/// Held in memory and written through on change, so reads never touch disk.
static STORE: Mutex<Option<Store>> = Mutex::new(None);

fn data_folder() -> PathBuf {
    DATA_FOLDER
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Runs `f` against the store, saving if it reports a change.
fn with_store<T>(f: impl FnOnce(&mut Store) -> (T, bool)) -> std::result::Result<T, String> {
    let mut guard = STORE.lock().map_err(|e| e.to_string())?;
    let store = guard.get_or_insert_with(Store::default);

    let (value, changed) = f(store);
    if changed {
        store.save(&data_folder())?;
    }
    Ok(value)
}

/// Splits a greedy argument into whitespace-separated pieces.
///
/// The command tree could name each argument separately, but one greedy string
/// lets reasons contain spaces without quoting — which is what a moderator
/// actually types.
fn words(args: &ConsumedArgs, key: &str) -> Vec<String> {
    match args.get_value(key) {
        Arg::Msg(text) | Arg::Simple(text) => text.split_whitespace().map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

fn reply(sender: &CommandSender, message: String) {
    sender.send_message(TextComponent::text(&message));
}

fn fail(message: &str) -> CommandError {
    CommandError::CommandFailed(TextComponent::text(message))
}

// ---------------------------------------------------------------------------
// /tempban
// ---------------------------------------------------------------------------

struct TempBan;

impl pumpkin_plugin_api::commands::CommandHandler for TempBan {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let parts = words(&args, "args");
        let (Some(target), Some(duration)) = (parts.first(), parts.get(1)) else {
            return Err(fail("Usage: /tempban <player> <duration> [reason]"));
        };

        let seconds = parse_duration(duration).map_err(|e| fail(&e))?;
        let reason = if parts.len() > 2 {
            parts[2..].join(" ")
        } else {
            "No reason given".to_string()
        };

        // The ban goes through the server's own ban list, so expiry, kicking and
        // persistence are all handled where they already work.
        let Some(player) = server.get_player_by_name(target) else {
            return Err(fail(&format!(
                "{target} is not online. Offline bans need a UUID lookup, which is not wired up yet."
            )));
        };

        let mut options = BanPlayerOptions::temporary(
            Some(TextComponent::text(&reason)),
            seconds,
        );
        options.source = Some(sender.get_name());
        player.ban(options);

        reply(
            &sender,
            format!(
                "Banned {target} for {}: {reason}",
                format_duration(seconds)
            ),
        );
        Ok(1)
    }
}

// ---------------------------------------------------------------------------
// /warn
// ---------------------------------------------------------------------------

struct Warn;

impl pumpkin_plugin_api::commands::CommandHandler for Warn {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let parts = words(&args, "args");
        let Some(target) = parts.first() else {
            return Err(fail("Usage: /warn <player> <reason>"));
        };
        if parts.len() < 2 {
            return Err(fail("A warning needs a reason."));
        }
        let reason = parts[1..].join(" ");

        let count = with_store(|store| {
            store.warnings.push(Warning {
                player: target.clone(),
                reason: reason.clone(),
                issued_by: sender.get_name(),
                issued_at: now(),
            });
            let count = store.history(target).len();
            (count, true)
        })
        .map_err(|e| fail(&format!("Could not save the warning: {e}")))?;

        reply(
            &sender,
            format!("Warned {target} ({count} total): {reason}"),
        );

        // A warning nobody sees is not a warning.
        if let Some(player) = server.get_player_by_name(target) {
            player.send_system_message(
                TextComponent::text(&format!("You were warned: {reason}")),
                false,
            );
        }

        Ok(1)
    }
}

// ---------------------------------------------------------------------------
// /history
// ---------------------------------------------------------------------------

struct History;

impl pumpkin_plugin_api::commands::CommandHandler for History {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let parts = words(&args, "args");
        let Some(target) = parts.first() else {
            return Err(fail("Usage: /history <player>"));
        };

        let lines = with_store(|store| {
            let rendered: Vec<String> = store
                .history(target)
                .iter()
                .take(20)
                .map(|w| format!("- {} by {}", w.reason, w.issued_by))
                .collect();
            (rendered, false)
        })
        .map_err(|e| fail(&format!("Could not read the history: {e}")))?;

        if lines.is_empty() {
            reply(&sender, format!("{target} has no warnings."));
        } else {
            reply(&sender, format!("Warnings for {target}:"));
            for line in lines {
                reply(&sender, line);
            }
        }
        reply(&sender, "Bans are in the server ban list; see /banlist.".to_string());
        Ok(1)
    }
}

/// Every command here takes one greedy argument and parses it itself.
fn greedy_arg() -> CommandNode {
    CommandNode::argument("args", &ArgumentType::String(StringType::Greedy))
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

struct PumpBans;

impl Plugin for PumpBans {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "PumpBans".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Dhanvin".into()],
            description: "Temporary bans with durations, plus player warnings.".into(),
            dependencies: vec![],
            // Only the plugin's own data folder. No network, no environment.
            permissions: vec![
                permissions::FS_READ_DATA.into(),
                permissions::FS_WRITE_DATA.into(),
            ],
        }
    }

    fn on_load(&mut self, context: Context) -> Result<()> {
        let folder = PathBuf::from(context.get_data_folder());
        let _ = std::fs::create_dir_all(&folder);
        let _ = DATA_FOLDER.set(folder.clone());

        let (store, problem) = Store::load(&folder);
        if let Some(problem) = problem {
            tracing::error!("{problem}; starting with no warnings loaded");
        }
        tracing::info!("loaded {} warning(s)", store.warnings.len());
        *STORE.lock().map_err(|e| e.to_string())? = Some(store);

        // `then` returns nothing, so the command is built in steps rather than
        // chained.
        let tempban = Command::new(
            &["tempban".to_string()],
            "Ban a player for a set time, e.g. /tempban Steve 2h griefing",
        );
        tempban.then(greedy_arg().execute(TempBan));
        context.register_command(tempban, "pumpbans:command.tempban");

        let warn = Command::new(&["warn".to_string()], "Warn a player");
        warn.then(greedy_arg().execute(Warn));
        context.register_command(warn, "pumpbans:command.warn");

        let history = Command::new(
            &["history".to_string(), "warnings".to_string()],
            "Show a player's warnings",
        );
        history.then(greedy_arg().execute(History));
        context.register_command(history, "pumpbans:command.history");

        tracing::info!("PumpBans ready");
        Ok(())
    }
}

register_plugin!(PumpBans);
