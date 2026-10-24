# Architecture

## Overview

```
Kestrel.exe (Rust, egui)                      Minecraft 1.8.9 (Java 8, separate process)
├─ kestrel-launcher  GUI, worker threads      ├─ LaunchWrapper + Forge 11.15.1.2318
└─ kestrel-core                               └─ kestrel-<ver>.jar (client mod)
   ├─ mojang/forge/assets/runtime  metadata      ├─ config      JSON config, presets
   ├─ download   verified parallel downloads     ├─ hud         modules, crosshair, editor
   ├─ install    end-to-end install + state      ├─ input       keybinds, CPS, toggle sprint
   ├─ launch     command line + process I/O      ├─ cosmetics   capes
   ├─ auth       Microsoft device-code flow      ├─ rendering   custom skies
   ├─ secure     DPAPI token storage             ├─ effects     kill detection + effects
   └─ config     launcher.json                   ├─ performance frame stats, video presets
                                                 └─ gui         settings screen, widgets
```

The launcher and the mod share exactly two things:

1. The game directory layout (`<root>/instance` is `--gameDir`; the mod keeps its data in
   `<root>/instance/kestrel`).
2. JVM properties: `-Dkestrel.requestedPreset=<potato|competitive|visual>` (applied by the
   mod once per distinct request) and the log marker `[Kestrel] Client initialized`, which
   the launcher watches for to confirm the mod loaded.

## Launcher

### Install pipeline (`install.rs`)

1. Version manifest → `1.8.9.json` (SHA-1 from the manifest).
2. Forge installer from `maven.minecraftforge.net`, verified against Forge's published
   `.sha1`; `install_profile.json` and the universal jar are read from the archive (no
   installer code is executed — 1.8.9 Forge has no processors).
3. Forge's `versionInfo` is merged onto vanilla (Forge libraries first; duplicates by
   `group:artifact[:classifier]`; Forge entries without hashes reuse vanilla's hashed entry
   for the same coordinate).
4. Client jar, libraries (rules evaluated for the OS; `clientreq: false` skipped; legacy Forge
   libraries try their Forge maven URL, then libraries.minecraft.net, then Maven Central, and
   accept any listed checksum), natives extracted (zip-slip safe, `META-INF/` excluded).
5. Asset index + objects, Mojang's log4j config.
6. Mojang's `jre-legacy` (Java 8) runtime, file by file with SHA-1.
7. The bundled mod jar is copied into `instance/mods` (only older `kestrel-*.jar` files are
   replaced).
8. `install-state.json` records classpath, main class, arguments, natives and file sizes.
   Play uses it after a cheap size check; **Repair** re-runs everything and re-hashes.

Downloads: fixed pool (default 8 threads), HTTPS only, **host allowlist** (Mojang, Forge,
Maven Central), `.part` file → verify size/SHA-1 → rename, 3 retries with backoff,
mirrors tried in order. An interrupted install never leaves a file that looks complete.

### Launch (`launch.rs`)

`build()` is a pure function (unit-tested) producing program + argument vector; the process is
started with `std::process::Command` (no shell, no string concatenation, spaces in paths are
safe). Output is read on two threads, redacted (access tokens and anything after
`--accessToken`), written to `logs/game-<time>.log` (+ `latest.log`) and streamed to the
Logs page.

### Auth (`auth.rs`)

Device code → token (polling with `slow_down` handling, cancellable) → Xbox Live → XSTS
(clear messages for known XErr codes) → Minecraft services → profile (404 = doesn't own
Java Edition). Only the refresh token and profile are persisted, DPAPI-encrypted; the
Minecraft token is fetched fresh at each launch.

## Packaging

`scripts/release.ps1` produces a single `Kestrel.exe`:

- The launcher is native Rust; all Rust dependencies are statically linked, and
  `launcher/.cargo/config.toml` links the MSVC C runtime statically (`+crt-static`).
- `crates/kestrel-launcher/build.rs` embeds the client mod jar given in
  `KESTREL_EMBED_MOD_JAR` (`include_bytes!`) and, for Windows targets, the icon/version
  resource. Fonts, the window icon and bundled cape/sky previews are `include_bytes!` too.
- At runtime `install::resolve_mod` writes the embedded jar once to
  `<root>/cache/client/` (only if its SHA-1 differs) and installs it into `instance/mods`.
  Lookup order: `%KESTREL_MOD_JAR%` → embedded → `<exe dir>/client/` (dev builds).
- Nothing is extracted to temporary folders; all state is under `%LOCALAPPDATA%\Kestrel`.
- `Kestrel.exe --self-check <file>` verifies the embedded mod (valid jar with `mcmod.info`
  and the mod class), unpacking, icon and every bundled preview, without opening a window.

### Graphics fallback

The launcher renders with wgpu by default (Direct3D 12 on Windows; adapters ranked discrete
GPU → integrated → virtual → software/WARP). If the renderer cannot start, the exe relaunches
itself with `--renderer glow --no-fallback` (winit allows one event loop per process). If that
fails too, a Windows error box explains why and where the log is. The working renderer is
remembered in `<root>/renderer.txt`; startup and adapter choice are always logged.

### Self-update

See [UPDATES.md](UPDATES.md): Ed25519-signed manifest on GitHub Releases, verified
download into `<root>/updates`, self-test of the new exe, rename-based swap of the
running exe, a health handshake with the restarted launcher, and automatic rollback.

## Cosmetics page (launcher ↔ client)

The launcher's **Cosmetics** page (kill effects, skies, capes) and the in-game menu edit the
same settings. Because the mod owns and rewrites `instance/kestrel/config.json`, the launcher
never writes it. Instead:

1. The launcher writes `instance/kestrel/launcher-cosmetics.json`
   (`kestrel-core/src/cosmetics.rs`): the cosmetic fields plus a strictly increasing
   `revision`, atomically.
2. The mod (`LauncherSync` + `config/CosmeticsRequest`) applies a revision exactly once: at
   start-up before anything renders, and while running by stat-ing the file once per second
   on its loader thread. It records `lastLauncherCosmeticsRevision` in config.json and saves.
3. The launcher displays the pending request until the mod's config shows it was applied,
   otherwise it displays the mod's config, so in-game changes appear in the launcher too.

Uploads are fully decoded and validated in Rust with the mod's exact rules, then copied (not
linked) into `instance/kestrel/capes` or `instance/kestrel/skies` under a sanitised file
name; identical re-uploads are de-duplicated by SHA-1. Previews are decoded on background
threads and cached as textures; bundled textures are embedded from the mod's resources so
the launcher and the game show the same art.

All cosmetics are **local**: rendered only by your client. Nothing is sent to servers, so
other players see them only if they run a compatible client that implements them.

## Client mod

Pure Forge events, no mixins yet (the Mixin infrastructure is configured for later
performance patches):

| Feature | Hook |
|---|---|
| HUD modules | `RenderGameOverlayEvent.Post(ALL)`; text rebuilt every `hudRefreshMs`, not every frame |
| Crosshair | `RenderGameOverlayEvent.Pre(CROSSHAIRS)` cancelled only when custom is on |
| Toggle sprint | `ClientTickEvent` + `KeyBinding.setKeyBindState`; vanilla sprint rules still apply |
| CPS | `MouseEvent` (in-game only; no global hooks) |
| Capes | `RenderPlayerEvent.Post`, local player only, replicating `RendererLivingEntity` transforms and vanilla `LayerCape` motion |
| Skies | Forge `WorldProvider#setSkyRenderer` (Overworld only; uninstalled when disabled) |
| Kill effects | `AttackEntityEvent` (who you hit) + `ClientTickEvent` (health ≤ 0) + `ClientChatReceivedEvent` (kill feed naming you and the victim) → `KillEffectManager` |
| Frame stats | `RenderTickEvent` (ring buffer, avg + 1% low) |

Threading: config is only touched on the game thread; saves are debounced (1 s), serialised
on the game thread and written atomically on one IO thread. Image decoding and folder scans
run on one loader thread; GL uploads happen on the game thread (≤ 4 per tick).

User images are validated before decoding: size limit, PNG signature, IHDR dimensions and
format rules (capes 2:1 frames, ≤ 2048 px tall; skies 3:2, ≤ 3072 px wide).

## Verification status (be honest about this)

Verified in the development environment (Linux, no access to Mojang/Forge/Gradle servers):

- Launcher core: 45 unit/integration tests pass (incl. cosmetics import/validation/sync), including resolution of the **real**
  1.8.9 version JSON, the **real** Forge 11.15.1.2318 install profile and Mojang's real Java
  runtime index (`launcher/crates/kestrel-core/tests`). `cargo fmt`, `clippy -D warnings` clean.
- Launcher, Windows target: type-checked with `cargo clippy --target x86_64-pc-windows-msvc
  -D warnings` (std built from source; TLS crate excluded because it needs a C compiler).
- Launcher GUI: renders (screenshot-checked under a virtual X display).
- Mod: every source file type-checks against deobfuscated Minecraft 1.8.9 (MCP 9.19) classes
  and Forge 1.8.9 sources, except three calls to Forge-patched `WorldProvider` sky methods
  (confirmed present in Forge's patch) and the template's shaded-ASM import.
- Cosmetics: launcher page screenshot-checked (tabs, grids, previews, invalid-file card,
  equip → request file); a Rust-written request was applied by the mod's Java code.
- Mod: 19 JUnit tests pass (config load/migrate/malformed handling, presets, image
  validation, frame stats, CPS, kill-message matching) against Minecraft's own Gson 2.2.4.

**Not yet verified** (needs a Windows machine with internet):

- A real Gradle build of the mod (architectury-loom remapping) and a real game launch.
- Real downloads from Mojang/Forge, the Java runtime install, Microsoft sign-in.
- Visual correctness of cape placement, skies and effects in game; FPS impact.
- The PowerShell scripts (written for Windows PowerShell 5.1; not executed yet).
