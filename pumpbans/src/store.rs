// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Dhanvin Raj Velagapudi
//! Warning records and their storage.
//!
//! Bans are deliberately *not* stored here. Pumpkin already keeps a ban list,
//! enforces it on join and persists it to `data/banned-players.json`, so
//! duplicating that would mean two sources of truth that quietly disagree.
//! Warnings have no equivalent in the server, so they live in the plugin's own
//! data folder.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Seconds since the Unix epoch.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    pub player: String,
    pub reason: String,
    pub issued_by: String,
    pub issued_at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub warnings: Vec<Warning>,
}

impl Store {
    fn path(data_folder: &Path) -> PathBuf {
        data_folder.join("warnings.json")
    }

    /// Reads the store, treating a missing file as empty.
    ///
    /// A corrupt file yields an empty store plus a message for the caller to
    /// log: refusing to load would take moderation offline over a stray comma.
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

    /// Warnings for a player, newest first. Names match case-insensitively,
    /// because nobody types capitals consistently under pressure.
    pub fn history(&self, player: &str) -> Vec<&Warning> {
        let mut found: Vec<&Warning> = self
            .warnings
            .iter()
            .filter(|w| w.player.eq_ignore_ascii_case(player))
            .collect();
        found.sort_by_key(|w| std::cmp::Reverse(w.issued_at));
        found
    }
}

/// Parses `30m`, `2h`, `7d` or `1w` into seconds.
pub fn parse_duration(input: &str) -> Result<u64, String> {
    let trimmed = input.trim().to_ascii_lowercase();

    let split = trimmed
        .find(|c: char| c.is_ascii_alphabetic())
        .ok_or_else(|| format!("'{input}' needs a unit, for example 30m, 2h or 7d"))?;
    let (amount, unit) = trimmed.split_at(split);

    // A duration starting with a letter, like "banana", would otherwise report
    // that the empty string is not a number.
    if amount.is_empty() {
        return Err(format!(
            "'{input}' needs a number before the unit, for example 2h"
        ));
    }

    let amount: u64 = amount
        .parse()
        .map_err(|_| format!("'{amount}' is not a number"))?;
    if amount == 0 {
        return Err("duration must be more than zero".to_string());
    }

    let seconds = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 60 * 60,
        "d" => 60 * 60 * 24,
        "w" => 60 * 60 * 24 * 7,
        other => return Err(format!("'{other}' is not a unit; use s, m, h, d or w")),
    };

    amount
        .checked_mul(seconds)
        .ok_or_else(|| "that duration is too long".to_string())
}

/// Renders seconds as something readable, to at most two units.
pub fn format_duration(seconds: u64) -> String {
    if seconds == 0 {
        return "no time".to_string();
    }

    let units = [
        ("w", 60 * 60 * 24 * 7),
        ("d", 60 * 60 * 24),
        ("h", 60 * 60),
        ("m", 60),
        ("s", 1),
    ];

    let mut left = seconds;
    let mut parts = Vec::new();
    for (label, size) in units {
        if left >= size {
            parts.push(format!("{}{label}", left / size));
            left %= size;
        }
        if parts.len() == 2 {
            break;
        }
    }

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("30m").unwrap(), 1800);
        assert_eq!(parse_duration("2h").unwrap(), 7200);
        assert_eq!(parse_duration("7d").unwrap(), 604_800);
        assert_eq!(parse_duration("1w").unwrap(), 604_800);
    }

    #[test]
    fn rejects_nonsense_durations() {
        assert!(parse_duration("soon").is_err());
        // Reports the missing number, not that "" failed to parse.
        assert!(parse_duration("banana")
            .unwrap_err()
            .contains("needs a number"));
        assert!(parse_duration("5x").is_err());
        assert!(parse_duration("0h").is_err());
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn formats_to_at_most_two_units() {
        assert_eq!(format_duration(9000), "2h 30m");
        assert_eq!(format_duration(60), "1m");
        assert_eq!(format_duration(694_861), "1w 1d");
    }

    #[test]
    fn history_is_newest_first_and_case_insensitive() {
        let store = Store {
            warnings: vec![
                Warning {
                    player: "Steve".into(),
                    reason: "first".into(),
                    issued_by: "admin".into(),
                    issued_at: 100,
                },
                Warning {
                    player: "steve".into(),
                    reason: "second".into(),
                    issued_by: "admin".into(),
                    issued_at: 200,
                },
            ],
        };

        let found = store.history("STEVE");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].reason, "second");
    }
}
