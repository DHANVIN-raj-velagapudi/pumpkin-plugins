// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Dhanvin Raj Velagapudi
//! homes — named homes for players.
//!
//! `/sethome [name]`, `/home [name]`, `/delhome [name]` and `/homes`. Leaving
//! the name off means a home called `home`, which is what players type nine
//! times out of ten.
//!
//! Homes are stored per player UUID in `homes.json` inside the plugin's data
//! folder, held in memory and written through on change. The only permissions
//! requested are read/write on that folder.

mod store;

use pumpkin_plugin_api::command::{
    Arg, ArgumentType, Command, CommandError, CommandNode, CommandSender, ConsumedArgs, StringType,
};
use pumpkin_plugin_api::commands::CommandHandler;
use pumpkin_plugin_api::permission::{Permission, PermissionDefault};
use pumpkin_plugin_api::text::TextComponent;
use pumpkin_plugin_api::{
    permissions, register_plugin, uuid, Context, Player, Plugin, PluginMetadata, Result, Server,
};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use store::{normalise_name, Home, SetOutcome, Store, DEFAULT_NAME, MAX_HOMES};

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

fn reply(sender: &CommandSender, message: &str) {
    sender.send_message(TextComponent::text(message));
}

/// Tells the sender what went wrong and ends the command normally.
///
/// Not `CommandError::CommandFailed`: the server wraps that in a parse
/// exception and prints it as `Syntax error: Unexpected "..."`.
fn refuse(sender: &CommandSender, message: &str) -> Result<i32, CommandError> {
    sender.send_error(TextComponent::text(message));
    Ok(0)
}

/// The optional `name` argument, normalised, or the default when it was left
/// off. An absent argument arrives as an empty string.
fn home_name(args: &ConsumedArgs) -> std::result::Result<String, String> {
    match args.get_value("name") {
        Arg::Simple(text) | Arg::Msg(text) if !text.trim().is_empty() => normalise_name(&text),
        _ => Ok(DEFAULT_NAME.to_string()),
    }
}

/// Homes belong to players, so the console has nothing to act on.
fn require_player(sender: &CommandSender) -> Option<Player> {
    let player = sender.as_player();
    if player.is_none() {
        sender.send_error(TextComponent::text("Only players have homes."));
    }
    player
}

fn key(player: &Player) -> String {
    uuid::to_string(player.get_id())
}

// ---------------------------------------------------------------------------
// /sethome
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SetHome;

impl CommandHandler for SetHome {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let Some(player) = require_player(&sender) else {
            return Ok(0);
        };
        let name = match home_name(&args) {
            Ok(name) => name,
            Err(problem) => return refuse(&sender, &problem),
        };

        let (x, y, z) = player.get_position();
        let home = Home {
            world: player.get_world().get_name(),
            x,
            y,
            z,
            yaw: player.get_yaw(),
            pitch: player.get_pitch(),
        };

        let who = key(&player);
        let saved = with_store(|store| match store.set(&who, &name, home) {
            Ok(outcome) => (Ok(outcome), true),
            Err(limit) => (Err(limit), false),
        });

        match saved {
            Ok(Ok(SetOutcome::Created)) => reply(&sender, &format!("Home '{name}' set.")),
            Ok(Ok(SetOutcome::Replaced)) => reply(&sender, &format!("Home '{name}' moved here.")),
            Ok(Err(limit)) => return refuse(&sender, &limit),
            Err(e) => return refuse(&sender, &format!("Could not save your home: {e}")),
        }
        Ok(1)
    }
}

// ---------------------------------------------------------------------------
// /home
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct GoHome;

impl CommandHandler for GoHome {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let Some(player) = require_player(&sender) else {
            return Ok(0);
        };
        let name = match home_name(&args) {
            Ok(name) => name,
            Err(problem) => return refuse(&sender, &problem),
        };

        let who = key(&player);
        let found = with_store(|store| (store.get(&who, &name).cloned(), false));
        let home = match found {
            Ok(Some(home)) => home,
            Ok(None) => {
                return refuse(
                    &sender,
                    &format!("You have no home called '{name}'. Try /homes."),
                )
            }
            Err(e) => return refuse(&sender, &format!("Could not read your homes: {e}")),
        };

        // A home can outlive its world, if the world was deleted or renamed.
        let Some(world) = server.get_world_by_name(&home.world) else {
            return refuse(
                &sender,
                &format!("The world for '{name}' ({}) no longer exists.", home.world),
            );
        };

        player.teleport_world(
            world,
            (home.x, home.y, home.z),
            Some(home.yaw),
            Some(home.pitch),
        );
        reply(&sender, &format!("Teleported to '{name}'."));
        Ok(1)
    }
}

// ---------------------------------------------------------------------------
// /delhome
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DelHome;

impl CommandHandler for DelHome {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let Some(player) = require_player(&sender) else {
            return Ok(0);
        };
        let name = match home_name(&args) {
            Ok(name) => name,
            Err(problem) => return refuse(&sender, &problem),
        };

        let who = key(&player);
        match with_store(|store| {
            let removed = store.delete(&who, &name);
            (removed, removed)
        }) {
            Ok(true) => reply(&sender, &format!("Home '{name}' removed.")),
            Ok(false) => return refuse(&sender, &format!("You have no home called '{name}'.")),
            Err(e) => return refuse(&sender, &format!("Could not update your homes: {e}")),
        }
        Ok(1)
    }
}

// ---------------------------------------------------------------------------
// /homes
// ---------------------------------------------------------------------------

struct ListHomes;

impl CommandHandler for ListHomes {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let Some(player) = require_player(&sender) else {
            return Ok(0);
        };

        let who = key(&player);
        let names = match with_store(|store| {
            (
                store
                    .names(&who)
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
                false,
            )
        }) {
            Ok(names) => names,
            Err(e) => return refuse(&sender, &format!("Could not read your homes: {e}")),
        };

        if names.is_empty() {
            reply(&sender, "You have no homes yet. Set one with /sethome.");
        } else {
            reply(
                &sender,
                &format!("Homes ({}/{MAX_HOMES}): {}", names.len(), names.join(", ")),
            );
        }
        Ok(1)
    }
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Declares a permission node granted to everyone by default.
///
/// A node that is never registered resolves to "denied" for everyone but the
/// console, so skipping this would make the commands unusable in game.
fn declare(context: &Context, node: &str, description: &str) -> Result<()> {
    context.register_permission(&Permission {
        node: node.into(),
        description: description.into(),
        default: PermissionDefault::Allow,
        children: Vec::new(),
    })
}

/// `/<name> [name]`: runs the handler with or without a home name.
fn named_command<H: CommandHandler + Clone + 'static>(
    command: &str,
    description: &str,
    handler: H,
) -> Command {
    Command::new(&[command.to_string()], description)
        .then(
            CommandNode::argument("name", &ArgumentType::String(StringType::SingleWord))
                .execute(handler.clone()),
        )
        .execute(handler)
}

struct Homes;

impl Plugin for Homes {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "homes".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Dhanvin".into()],
            description: "Named homes: /sethome, /home, /delhome and /homes.".into(),
            dependencies: vec![],
            // Only the plugin's own data folder. No network, no environment.
            permissions: vec![
                permissions::FS_READ_DATA.into(),
                permissions::FS_WRITE_DATA.into(),
            ],
        }
    }

    fn on_load(&self, context: Context) -> Result<()> {
        let folder = PathBuf::from(context.get_data_folder());
        let _ = std::fs::create_dir_all(&folder);
        let _ = DATA_FOLDER.set(folder.clone());

        let (store, problem) = Store::load(&folder);
        if let Some(problem) = problem {
            tracing::error!("{problem}; starting with no homes loaded");
        }
        tracing::info!("loaded homes for {} player(s)", store.players.len());
        *STORE.lock().map_err(|e| e.to_string())? = Some(store);

        for (node, description) in [
            ("homes:command.sethome", "Set a home"),
            ("homes:command.home", "Teleport to a home"),
            ("homes:command.delhome", "Remove a home"),
            ("homes:command.homes", "List your homes"),
        ] {
            declare(&context, node, description)?;
        }

        context.register_command(
            named_command("sethome", "Set a home where you stand", SetHome),
            "homes:command.sethome",
        );
        context.register_command(
            named_command("home", "Teleport to one of your homes", GoHome),
            "homes:command.home",
        );
        context.register_command(
            named_command("delhome", "Remove one of your homes", DelHome),
            "homes:command.delhome",
        );
        context.register_command(
            Command::new(&["homes".to_string()], "List your homes").execute(ListHomes),
            "homes:command.homes",
        );

        tracing::info!("homes ready");
        Ok(())
    }
}

register_plugin!(Homes);
