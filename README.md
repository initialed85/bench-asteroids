# benchy

A tiny multiplayer asteroids. The client is Bevy 0.19 compiled to WASM (WebGL2,
immediate-mode gizmos); the server is plain Rust that owns the simulation. No
trunk, no tokio, no asset pipeline, no font atlas — the whole client is lines
and circles over a JSON protocol, so the build stays as small as Bevy allows.

## Layout

| path | what |
| --- | --- |
| `common/` | protocol types shared by client and server (serde) |
| `server/` | native binary: authoritative sim + HTTP/WS + static files |
| `client/` | Bevy app compiled to `wasm32-unknown-unknown` |
| `web/` | `index.html`, `app.js`, and the wasm-bindgen output |

## Build

```sh
./build.sh
```

Prerequisites: rust, the `wasm32-unknown-unknown` target, `curl`.

`build.sh` fetches the `wasm-bindgen` 0.2.129 CLI into `tools/` on first run.
The CLI version must match the pinned `wasm-bindgen = "=0.2.129"` in
`client/Cargo.toml`; upstream publishes no glibc asset for that release, so the
musl build is used.

Success looks like: `web/benchy_client.js` + `web/benchy_client_bg.wasm` and
`target/release/benchy-server`.

`build.sh` strips the wasm `name` section (`--remove-name-section`, ~35 MB) and
writes a gzip sibling `benchy_client_bg.wasm.gz`; the server serves it with
`Content-Encoding: gzip` when the client accepts gzip. Raw 44 MB → 9 MB on the
wire. Bevy's render stack is the bulk of it; a gizmo-only client cannot avoid
the renderer.

## Run

```sh
./run.sh            # http://localhost:11111
```

Open two tabs; `?name=alice` sets the player name. The HUD shows score, deaths,
tick number and how stale the last snapshot is.

Verified end to end with headless chromium: HUD reports `connected`, two tabs
each see `2 players`, shooting a rock raises the score, thrusting into one
raises `deaths` and triggers respawn.

## How it works

- The server ticks at 20 Hz and broadcasts one JSON snapshot per client.
- The client sends input only when it changes, plus a ping every 5 s.
- The server polls each socket with a nonblocking read (`WouldBlock`), so one
  thread per connection is enough; the tick thread hands snapshots over through
  an `mpsc` channel.
- The client exponentially smooths positions between snapshots, so 20 Hz looks
  like 60 fps.
- Rendering is `Gizmos` (lines + `circle_2d`); the HUD is a DOM element, not
  Bevy text.

## Game rules

- 1600x900 arena, bounce off the walls.
- Asteroid sizes 3 → 2 → 1, radius 60 / 38 / 22; a shot splits a big rock into
  two smaller ones, size 1 dies.
- Ship: thrust 260 u/s², drag 0.6/s, max 340 u/s, turn 3.2 rad/s.
- Bullets: 620 u/s, 1.1 s life, 0.28 s cooldown, max 4 in flight.
- Score 20 / 40 / 60 for size 3 / 2 / 1; death costs a point, respawn after
  1.5 s with 2 s of invulnerability.

## Traps

- `tungstenite` 0.24 has no `split()` and no read timeout, so the handshake must
  be done manually (SHA-1 + base64 over `Sec-WebSocket-Key`) and the header read
  byte-by-byte — a `BufReader` would swallow the first WS frame.
- Bevy 0.19 renamed `Input<T>` to `ButtonInput<T>`, `Color::srgb` is gone in
  favour of `Color::srgb_u32`, and `WindowResolution::new` takes `u32`.
- `bevy` 0.19 no longer exposes `bevy_input`/`bevy_time` as features; they are
  unconditional dependencies of `bevy_internal`, so the feature list is just
  `2d`, `web`, `std`, `async_executor`, `bevy_color`, `bevy_log`,
  `bevy_window`, `bevy_winit`.
- The client must be a `cdylib` for wasm-bindgen, and `web-sys` needs the
  `WebSocket`, `MessageEvent`, `Document`, `Element`, `Location`, `EventTarget`
  and `console` features.
