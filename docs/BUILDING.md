# Building Deus Vult shells

The game client core (`crusade-app`) is frontend-agnostic: it compiles to
plain Rust with no native deps. The visual shell is the `crusade-desktop`
Bevy binary in the same crate, gated behind the Cargo feature `bevy-ui`
(so headless/CI builds of the client never pay for a graphics stack).

> **Submodule gate.** `deps/cdk` (vendored cashu/cdk path deps) is a git
> submodule; uninitialized, every cargo build/test fails with `error:
> failed to get 'cashu' as a dependency`. Fresh clones must run
> `git submodule update --init deps/cdk` (or clone `--recurse-submodules`)
> before any cargo/trunk command.

## (a) Desktop

```console
$ cargo run -p crusade-app --features bevy-ui --bin crusade-desktop
```

Opens a window titled "Deus Vult" (menu, rival roster, practice duel at
level 25, seven-sins campaign, battle log chronicle). A fresh in-memory
roster with a starter band of six champions (species 1–6, level 25) is
seeded each launch.

Native prerequisites (Ubuntu names): `pkg-config`, `libudev-dev`, and X11 or
Wayland dev libraries (`libx11-dev`/`libxkbcommon-dev` or
`libwayland-dev`).

> **No audio in CI builds.** The crate pins bevy 0.18's default feature set
> expanded and *minus* `audio`: `bevy_audio` pulls `cpal -> alsa-sys`, which
> needs the system ALSA dev package (`libasound2-dev`), unavailable in CI.
> On a developer machine with ALSA installed you may switch the dep to
> `bevy = { version = "0.18", optional = true }` (full defaults) to get
> sound support back; the shell code does not care either way.

### Smoke harness (CI-safe)

```console
$ cargo run -p crusade-app --features bevy-ui --bin crusade-desktop -- --smoke
```

Runs 120 frames of a scripted practice duel (rite 0 each turn, with the
test-fixture fallbacks so zeal exhaustion can't stall) and exits 0,
printing one summary line:

```
crusade-desktop smoke: frames=120 turns=32 battles_done=1 phase=Menu turn=0
```

`--smoke` always swaps `DefaultPlugins` for `MinimalPlugins` (unthrottled
loop): the shell systems and game code are identical, but no window or GPU
is required. The same swap happens automatically for the plain binary when
neither `DISPLAY` nor `WAYLAND_DISPLAY` is set — on such a display-less
host bevy's window/render setup would otherwise panic.

## (b) Web (wasm32 + trunk)

### Play in browser (PWA)

Every push to `master` that touches `crusade-*/` rebuilds and deploys the
game to this repo's GitHub Pages site (`.github/workflows/web-pages.yml`):

```
https://<owner>.github.io/kirk/
```

It is a fully offline-capable PWA: a service worker precaches the whole
wasm/js bundle on first load, so once a visit has completed the game runs
with no network at all. **Install it** for the app-like experience:
Chrome/Edge show an install icon in the address bar ("Install Deus Vult"),
Safari uses Share -> Add to Home Screen; it then launches standalone with
the crusader-shield icon. Progress (roster/champions) persists in the
browser's localStorage under the key `deusvult.roster.v1`; clearing site
data wipes the save.

What v1 includes, honestly:

- Practice duels vs the deterministic AI team — offline.
- The seven-sins campaign gauntlet, incl. rest/rewards flow — offline.
- Roster persistence across reloads/devices-with-the-same-browser.
- **PvP is NOT wired into the UI yet** (the net driver exists in
  `crusade-app`'s library, but no screen drives it).
- **Wagers/ecash are deferred** (no Cashu flows in this UI).
- No audio (bevy `audio` feature is excluded everywhere; see below).

Local PWA run:

```console
$ cd crusade-app/web
$ trunk serve --release
```

then open the printed localhost URL (service workers need a secure context;
`localhost` counts). The production file set — `index.html`, hashed
`*_bg.wasm` + `*.js`, `manifest.webmanifest`, `sw.js`, `icon.svg` +
`icon-192.png`/`icon-512.png` — lands in
`crusade-app/web/dist/`; build it standalone with `trunk build --release`
(add `--public-url /kirk/` when targeting Pages).

One-time setup for building locally — the submodule gate applies: run
`git submodule update --init deps/cdk` before the first `trunk` command,
then:

```console
$ rustup target add wasm32-unknown-unknown
$ cargo install trunk
```

The trunk entry point is [`crusade-app/web/index.html`](../crusade-app/web/index.html);
it builds the same `crusade-desktop` bin with `--features bevy-ui` and
injects the wasm-bindgen loader. Serve it with:

```console
$ cd crusade-app/web
$ trunk serve --release
```

then open the printed localhost URL. **Always use `--release`**: a debug
build of bevy on wasm is unusably slow. Trunk's `wasm-bindgen-cli` must
match the `wasm-bindgen` version bevy pulls in; if binding fails, run
`cargo install wasm-bindgen-cli --version <matching>` or let trunk resolve
it (it does automatically when left alone).

Current status of a bare wasm target check:

```console
$ cargo check --target wasm32-unknown-unknown -p crusade-app --features bevy-ui --bin crusade-desktop
```

**Status: compiles cleanly (verified 2026-10-14, bevy 0.18.1).** The
entire dependency tree — bevy's render/winit/ui stacks, crusade-net's
nostr, crusade-data's automerge — checks for `wasm32-unknown-unknown`
with no blockers.

## (c) Mobile (iOS / Android)

Bevy has growing mobile support, but these builds need platform SDKs that
are not available in CI — treat them as developer-machine work:

- **Android**: `rustup target add aarch64-linux-android` (already installed
  here) plus the Android NDK/SDK, then follow bevy's official *Android /
  iOS* setup docs (`bevy`'s `android-game-activity` path). Practically:
  use [`cargo-mobile2`](https://github.com/tauri-apps/cargo-mobile2)
  (`cargo install cargo-mobile2`, `cargo mobile init`, `cargo mobile
  run --target android`) with the `crusade-desktop` entry point and the
  `bevy-ui` feature.
- **iOS**: `rustup target add aarch64-apple-ios`, Xcode + a signing
  identity, then `cargo mobile run --target ios` via the same
  cargo-mobile2 flow. The simulator target is
  `aarch64-apple-ios-sim`.

No CI gate exists for either; verify on SDK-equipped hardware before
claiming support.
