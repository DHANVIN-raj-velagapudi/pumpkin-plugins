# Pumpkin Plugins

Plugins for [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin) Minecraft servers, built against its
WebAssembly plugin API.

## Download

Prebuilt `.wasm` files are attached to every [Release](../../releases) — no toolchain required.

- **Stable**: pick a tagged version, e.g. `v0.1.0`.
- **Latest**: the [`latest` pre-release](../../releases/tag/latest) rebuilds automatically from
  every push to `main`.

To install one, drop its `.wasm` file into your server's `plugins/` folder and restart. The server
will list the permissions it requests and ask for confirmation before loading it.

## Plugins

| Plugin | What it does | Permissions requested |
| --- | --- | --- |
| [`pumpbans`](pumpbans) | `/tempban` with a real duration (`2h`, `7d`, ...), plus `/warn` and `/history` for a player warning log. | own data folder |
| [`homes`](homes) | `/sethome`, `/home`, `/delhome` and `/homes`: up to five named homes per player. | own data folder |
| [`panel-bridge`](panel-bridge) | Writes live server status (TPS, MSPT, players) to a JSON file for external tools such as [pumpkin-panel](https://github.com/DHANVIN-raj-velagapudi/pumpkin-panel). | write to own data folder |
| [`hello-pumpkin`](hello-pumpkin) | Minimal example plugin — logs on load and unload. Not something you'd run; a template to build from. | none |

### In-game permissions

Command access is controlled by permission nodes, namespaced by plugin name.

| Node | Default |
| --- | --- |
| `PumpBans:command.tempban` | operators, level 3 |
| `PumpBans:command.warn`, `PumpBans:command.history` | operators, level 2 |
| `homes:command.sethome`, `.home`, `.delhome`, `.homes` | everyone |

### `panel-bridge` status file

`plugins/data/panel-bridge/status.json`, refreshed about once a second:

```json
{
  "schema": 1,
  "state": "running",
  "updated_at": 1791119599,
  "server": {
    "tps": 20.0, "mspt": 0.1,
    "players_online": 0, "max_players": 1000,
    "motd": "A blazingly fast Pumpkin server!",
    "online_mode": true, "hardcore": false,
    "difficulty": "normal", "whitelist": false
  },
  "players": [
    { "name": "Steve", "uuid": "…", "world": "minecraft:overworld",
      "gamemode": "survival", "ping_ms": 23, "health": 20.0 }
  ]
}
```

- `state` is `"stopping"` after a clean shutdown. After a crash it stays `"running"` while
  `updated_at` stops advancing, so a reader can tell the two apart.
- `tps` is capped at 20. Pumpkin computes it from tick duration with no ceiling, so an idle server
  would otherwise report thousands.
- `schema` is bumped only when a field is removed or changes meaning.

## Building from source

Requires Rust and the `wasm32-wasip2` target:

```bash
rustup target add wasm32-wasip2
```

The plugin API is published on crates.io as `pumpkin-plugin-api`, pinned in the workspace
[`Cargo.toml`](Cargo.toml) to the server version it targets, so nothing else needs checking out.

```bash
cargo build --release --target wasm32-wasip2 --workspace
```

Built components land in `target/wasm32-wasip2/release/*.wasm`.

The pure logic (stores, parsing, JSON layout) is tested natively; the glue that calls into the
server needs a running server to exercise:

```bash
cargo test --workspace --lib
```

See [`docs/pumpkin-plugin-notes.md`](docs/pumpkin-plugin-notes.md) for API behaviour that is not
obvious from the docs.

## Licence

[GPL-3.0-only](LICENSE), the same licence as the Pumpkin server.

You can run, modify and redistribute these plugins, but a modified version you distribute must be
offered under the GPL with its source. Running a modified copy on your own server, without handing
it to anyone, carries no obligation. The Pumpkin plugin API these are built on is separately
licensed `MIT OR Apache-2.0`, which is compatible.
