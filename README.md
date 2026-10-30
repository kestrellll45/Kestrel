# Kestrel

A Minecraft Java Edition **1.8.9** PvP client: a native Windows launcher (Rust) plus a
Forge client mod (Java) with a configurable HUD, custom crosshair, toggle sprint, local
capes, custom skies, kill effects and performance presets.

- **Zoom** (hold C) and **360° free-look** (hold Left Alt): camera-only, rebindable, see
  [documentation/ZOOM_FREELOOK.md](documentation/ZOOM_FREELOOK.md).
- **Local skin changer** in the launcher (Skin page) and on the Minecraft main menu: import a
  PNG or load a NameMC profile link. Shown only in your own client; your account skin is not
  changed. See [documentation/SKINS.md](documentation/SKINS.md).

Kestrel keeps your field of view fixed: sprinting, Speed/Slowness, flying, drawing a
bow, dying, being under water and other mods' FOV events no longer zoom the camera.
Your FOV slider in Video Settings works as usual (`client/.../rendering/StaticFov.java`);
hold-to-zoom is applied on top of it for the current frame only.

```
launcher/   Rust workspace: kestrel-core (install/verify/auth/launch/update), kestrel-launcher (egui GUI),
            kestrel-release (publisher signing tool, not shipped)
client/     Forge 1.8.9 mod (Gradle, architectury-loom)
scripts/    setup, build, test, release, setup-updates, bump-version, publish-release (.ps1)
tools/      gen_assets.py, gen_cosmetics.py (generate bundled cape/sky textures, thumbnails, catalogs)
documentation/
```

Quick start (Windows, PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -File scripts\setup.ps1 -Yes   # installs Rust, MSVC build tools, JDK 17 if missing
powershell -ExecutionPolicy Bypass -File scripts\build.ps1 -Run   # builds mod + launcher, starts Kestrel
```

**Standalone release** (one self-contained `Kestrel.exe`):

```powershell
powershell -ExecutionPolicy Bypass -File scripts\release.ps1   # -> release\Kestrel.exe
```

## QoL tab

Manage external Forge 1.8.9 mods (install, enable/disable, remove with recovery),
resource packs (enable, disable, reorder, preview; saved to `options.txt` like
Minecraft's own menu) and toggle **Fullbright**, all from the launcher. See
[documentation/QOL.md](documentation/QOL.md).

## Automatic updates

`Kestrel.exe` checks [GitHub Releases](https://github.com/kestrellll45/Kestrel/releases)
at start-up and offers new versions in a window (**Update Now** / **Later**). It verifies
each one (Ed25519-signed manifest, SHA-256, self-test), swaps it in, and restarts. Your
data in `%LOCALAPPDATA%\Kestrel` is kept.

Download the latest `Kestrel.exe` from the Releases page once; after that it updates itself.

Publishing a version: `scripts\bump-version.ps1 -Version x.y.z`, write `notes\x.y.z.md`,
commit and push, then `scripts\publish-release.ps1 -Notes notes\x.y.z.md` (needs the
private signing key). Details, security notes and key backup are in
[documentation/UPDATES.md](documentation/UPDATES.md).

See [documentation/BUILD.md](documentation/BUILD.md) for details, including Microsoft sign-in setup.

Game data lives in `%LOCALAPPDATA%\Kestrel` and never touches your normal `.minecraft`.
Not affiliated with Mojang or Microsoft.
