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

| Plugin | What it does |
| --- | --- |
| [`pumpbans`](pumpbans) | `/tempban` with a real duration (`2h`, `7d`, ...), plus `/warn` and `/history` for a player warning log. |
| [`hello-pumpkin`](hello-pumpkin) | Minimal example plugin — logs on load and unload. Not something you'd run; a template to build from. |

## Building from source

Requires Rust and the `wasm32-wasip2` target:

```bash
rustup target add wasm32-wasip2
```

Pumpkin's plugin API is not published to crates.io, so it's built directly from a checkout of the
server. Its WIT interface definitions are a git submodule, so a shallow clone alone leaves them
empty:

```bash
git clone --recurse-submodules https://github.com/Pumpkin-MC/Pumpkin ../Pumpkin-src
```

Then, from this directory:

```bash
cargo build --release --target wasm32-wasip2 --workspace
```

Built components land in `target/wasm32-wasip2/release/*.wasm`.

## Licence

MIT.
