//! Home records and their storage.
//!
//! Everything here is plain data and rules, with no host calls, so the limits
//! and the file handling can be tested natively.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Homes a player may hold at once. Replacing an existing one is always
/// allowed; only a *new* name counts against the limit.
pub const MAX_HOMES: usize = 5;

/// The name used when a command is given none, so `/sethome` and `/home` work
/// the way most players expect.
pub const DEFAULT_NAME: &str = "home";

const MAX_NAME_LEN: usize = 24;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Home {
    /// The world's name as the server reports it, looked up again on use. A
    /// name rather than a handle, because handles do not survive a restart.
    pub world: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    /// Keyed by player UUID, not name: names can be changed, UUIDs cannot, and
    /// a player who renames must not lose their homes.
    #[serde(default)]
    pub players: BTreeMap<String, BTreeMap<String, Home>>,
}

/// What `set` did, so the caller can word the reply.
#[derive(Debug, PartialEq, Eq)]
pub enum SetOutcome {
    Created,
    Replaced,
}

impl Store {
    fn path(data_folder: &Path) -> PathBuf {
        data_folder.join("homes.json")
    }

    /// Reads the store, treating a missing file as empty.
    ///
    /// A corrupt file yields an empty store plus a message to log. Refusing to
    /// load would disable `/home` for everyone over one stray comma; the
    /// damaged file is left on disk untouched until the next save.
    pub fn load(data_folder: &Path) -> (Self, Option<String>) {
        let path = Self::path(data_folder);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return (Self::default(), None);
        };

        match serde_json::from_str(&text) {
            Ok(store) => (store, None),
            Err(e) => (
                Self::default(),
                Some(format!("{} could not be parsed: {e}", path.display())),
            ),
        }
    }

    pub fn save(&self, data_folder: &Path) -> Result<(), String> {
        let path = Self::path(data_folder);
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;

        // Written beside the target then renamed, so an interrupted save cannot
        // leave a half-written file behind.
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&temp, &path).map_err(|e| e.to_string())
    }

    pub fn set(&mut self, player: &str, name: &str, home: Home) -> Result<SetOutcome, String> {
        let homes = self.players.entry(player.to_owned()).or_default();
        let exists = homes.contains_key(name);

        if !exists && homes.len() >= MAX_HOMES {
            return Err(format!(
                "You already have {MAX_HOMES} homes. Remove one with /delhome first."
            ));
        }

        homes.insert(name.to_owned(), home);
        Ok(if exists {
            SetOutcome::Replaced
        } else {
            SetOutcome::Created
        })
    }

    pub fn get(&self, player: &str, name: &str) -> Option<&Home> {
        self.players.get(player)?.get(name)
    }

    /// Returns whether a home was actually removed.
    pub fn delete(&mut self, player: &str, name: &str) -> bool {
        let Some(homes) = self.players.get_mut(player) else {
            return false;
        };
        let removed = homes.remove(name).is_some();

        // Don't leave an empty entry behind for every player who ever used it.
        if homes.is_empty() {
            self.players.remove(player);
        }
        removed
    }

    /// Home names for a player, alphabetical.
    pub fn names(&self, player: &str) -> Vec<&str> {
        self.players
            .get(player)
            .map(|homes| homes.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }
}

/// Normalises a name typed by a player: trimmed, lower-cased, and restricted
/// to characters that are safe to show, store and type back.
pub fn normalise_name(input: &str) -> Result<String, String> {
    let name = input.trim().to_ascii_lowercase();

    if name.is_empty() {
        return Err("A home needs a name.".to_string());
    }
    if name.len() > MAX_NAME_LEN {
        return Err(format!(
            "Home names can be at most {MAX_NAME_LEN} characters."
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Home names may only use letters, digits, '_' and '-'.".to_string());
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spot(x: f64) -> Home {
        Home {
            world: "minecraft:overworld".into(),
            x,
            y: 64.0,
            z: 0.0,
            yaw: 90.0,
            pitch: 0.0,
        }
    }

    #[test]
    fn names_are_normalised() {
        assert_eq!(normalise_name("  Base ").unwrap(), "base");
        assert_eq!(normalise_name("mine-2_a").unwrap(), "mine-2_a");
    }

    #[test]
    fn bad_names_are_rejected() {
        assert!(normalise_name("").is_err());
        assert!(normalise_name("   ").is_err());
        assert!(normalise_name("has space").is_err());
        assert!(normalise_name("§color").is_err());
        assert!(normalise_name(&"a".repeat(25)).is_err());
        assert!(normalise_name(&"a".repeat(24)).is_ok());
    }

    #[test]
    fn set_get_and_replace() {
        let mut store = Store::default();
        assert_eq!(store.set("p", "base", spot(1.0)), Ok(SetOutcome::Created));
        assert_eq!(store.set("p", "base", spot(2.0)), Ok(SetOutcome::Replaced));
        assert_eq!(store.get("p", "base").unwrap().x, 2.0);
        assert!(store.get("p", "missing").is_none());
        assert!(store.get("someone-else", "base").is_none());
    }

    #[test]
    fn limit_blocks_new_homes_but_not_replacing() {
        let mut store = Store::default();
        for n in 0..MAX_HOMES {
            store.set("p", &format!("h{n}"), spot(n as f64)).unwrap();
        }
        assert!(store.set("p", "one-too-many", spot(0.0)).is_err());
        // An existing name can still be moved at the limit.
        assert_eq!(store.set("p", "h0", spot(9.0)), Ok(SetOutcome::Replaced));
        // The limit is per player.
        assert!(store.set("q", "h0", spot(0.0)).is_ok());
    }

    #[test]
    fn delete_removes_and_tidies_up() {
        let mut store = Store::default();
        store.set("p", "a", spot(0.0)).unwrap();
        assert!(store.delete("p", "a"));
        assert!(!store.delete("p", "a"));
        assert!(!store.players.contains_key("p"));
    }

    #[test]
    fn names_are_alphabetical() {
        let mut store = Store::default();
        for name in ["zeta", "alpha", "mid"] {
            store.set("p", name, spot(0.0)).unwrap();
        }
        assert_eq!(store.names("p"), ["alpha", "mid", "zeta"]);
        assert!(store.names("nobody").is_empty());
    }

    #[test]
    fn round_trips_through_disk_and_survives_a_corrupt_file() {
        let folder = std::env::temp_dir().join(format!("homes-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();

        let mut store = Store::default();
        store.set("p", "base", spot(5.0)).unwrap();
        store.save(&folder).unwrap();

        let (loaded, problem) = Store::load(&folder);
        assert!(problem.is_none());
        assert_eq!(loaded.get("p", "base"), Some(&spot(5.0)));

        std::fs::write(folder.join("homes.json"), "{ not json").unwrap();
        let (empty, problem) = Store::load(&folder);
        assert!(problem.is_some());
        assert!(empty.players.is_empty());

        std::fs::remove_dir_all(&folder).unwrap();
    }
}
