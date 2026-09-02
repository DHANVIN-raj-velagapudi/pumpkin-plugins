//! Smallest possible Pumpkin plugin, used to prove the toolchain end to end.

use pumpkin_plugin_api::{register_plugin, Context, Plugin, PluginMetadata, Result};

struct HelloPumpkin;

impl Plugin for HelloPumpkin {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "hello-pumpkin".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Dhanvin".into()],
            description: "Proves the plugin build and load pipeline works.".into(),
            dependencies: vec![],
            // No capabilities requested: this plugin only writes to the log.
            permissions: vec![],
        }
    }

    fn on_load(&mut self, _context: Context) -> Result<()> {
        tracing::info!("hello-pumpkin loaded");
        Ok(())
    }

    fn on_unload(&mut self, _context: Context) -> Result<()> {
        tracing::info!("hello-pumpkin unloaded");
        Ok(())
    }
}

register_plugin!(HelloPumpkin);
