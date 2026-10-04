# Pumpkin plugin API: things the docs don't say

Found by building against `pumpkin-plugin-api 0.2.0` (server `0.2.0+26.3-26.51`) and reading the
server source. The API is explicitly unstable, so re-check these when bumping the pinned version.

## Setup

- `pumpkin-plugin-api` is on crates.io and its version tracks the server's. Pin it exactly
  (`=0.2.0`). The server's own docs still show an older version, and this repo's earlier README
  claimed it was unpublished.
- Build with `--target wasm32-wasip2`. Don't set it as the default target in `.cargo/config.toml`:
  that makes `cargo test` try to run wasm. Passing it explicitly lets the pure logic run natively.
- Crate type `["cdylib", "rlib"]`: `cdylib` for the server, `rlib` so tests link.

## Lifecycle and builders

- `on_load` / `on_unload` take `&self`, not `&mut self`. State lives in a `static`
  (`Mutex` / `OnceLock`).
- Builders **consume and return** `self`: `Command::new(..).then(node).execute(handler)`,
  `TextComponent::text(..).color_named(..)`. A statement like `cmd.then(x);` moves `cmd` and loses
  the result.
- `TextComponent` styling methods also consume. Prefer `sender.send_error(..)` over colouring red
  by hand.

## Commands

- An **absent optional argument** comes back from `get_value` as `Arg::Simple("")`, not an error.
- A root node with no `.execute(..)` answers "Unknown command" when typed bare. Add a usage handler.
- **`CommandError::CommandFailed` is rendered as a parse error** (`Syntax error: Unexpected "..."`)
  because the host wraps it in a parse exception. For runtime failures send
  `sender.send_error(..)` and return `Ok(0)`.

## Permissions

- A permission node must be registered with `context.register_permission(..)` before use. An
  **unregistered node is denied to everyone except the console**, including operators.
- The node's namespace must be the plugin's **metadata name**, case included (`PumpBans:...`). A
  mismatch is refused and the whole plugin fails to initialise.
- A bare node passed to `register_command` is auto-prefixed with the plugin name.
- Defaults: `PermissionDefault::Allow`, `Deny`, or `Op(level)`.

## Data and permissions the server asks for

- A plugin's data folder is `plugins/data/<metadata name>`; it needs `fs.read.data` /
  `fs.write.data`. Renaming a plugin therefore orphans its data.
- Write-then-rename inside the folder works under WASI and is how both stores avoid torn files.

## Server behaviour

- `Server::get_tps()` has no ceiling; an idle server reports ~10,000. Clamp to 20 for display.
- There is no offline name → UUID lookup, so a never-seen player can't be banned by name.
  `get_ban_manager().ban_player(name, uuid, ..)` works for offline players *if you have the UUID*.

## Testing without a client

Copy the server binary and `pumpkin.toml` to a scratch folder, move the ports, disable telemetry,
list the plugin's permissions in `[plugins] allowed_permissions` (to skip the confirmation prompt),
and pipe console commands in:

```bash
(sleep 6; echo plugins; echo "warn Steve test"; sleep 2; echo stop) | ./pumpkin.exe
```

This exercises load, registration, permissions and console commands. Player-only paths (`/home`
teleporting, in-game permission checks) need a real client.
