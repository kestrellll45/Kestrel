# QoL tab

The launcher's **QoL** tab (left navigation, under Cosmetics) has three sections:
**Mod Manager**, **Resource Packs** and **Fullbright**. Everything works on Kestrel's own
game directory, `%LOCALAPPDATA%\Kestrel\instance` (the folder Minecraft is started with
via `--gameDir`); your normal `.minecraft` is never touched.

| Change | When it takes effect |
|---|---|
| Install / enable / disable / remove a mod | Next time Minecraft starts |
| Enable / disable / reorder resource packs | Next time Minecraft starts |
| Fullbright on/off | Within about a second if Minecraft is running, otherwise at the next Play |

While Minecraft is starting or running, mod and resource-pack controls are disabled:
Forge keeps mod jars open, and Minecraft rewrites `options.txt` from memory, which
would undo changes made from outside. (If you closed the launcher while the game kept
running, close the game before changing mods or packs.)

## Mod Manager

* **List.** Shows every `.jar`/`.zip` in `instance\mods` and `instance\mods\1.8.9` (the
  two places Forge 1.8.9 loads mods from) plus disabled mods. Each entry shows the
  name and version from the mod's `mcmod.info` when present (otherwise the file
  name), the file name and size, and its state. Sort by name or status.
* **Compatibility** is judged from the file, never from the `.jar` extension alone:
  * *Supported*: `mcmod.info` declares Minecraft 1.8.9 (or `1.8`).
  * *Unverified*: looks like a Forge mod but declares no version (no `mcmod.info`, a
    core mod, or an unrecognised value). It can be installed after a confirmation.
  * *Incompatible*: declares another Minecraft version, or is a Fabric, Quilt,
    LiteLoader or NeoForge mod, a modern Forge (`mods.toml`) mod, a Bukkit plugin, or a
    resource pack. Install is refused.
  * *Broken file*: not a readable JAR/ZIP archive.
* **Install mod…** opens a file picker. The file is checked (valid archive, size limit
  256 MB, not empty, compatibility, same file name or same mod id already installed),
  copied to a temporary name Forge ignores, size-verified, then moved into
  `instance\mods` only if that name is free. Nothing is ever overwritten.
* **Disable** moves the file to `%LOCALAPPDATA%\Kestrel\mods-disabled` (outside the
  game folder, so Forge can't see it). **Enable** moves it back to `instance\mods`; it
  is refused if a file with that name is already active or the same mod id is already
  enabled (Forge can't load a mod twice). If the disabled folder already has a file
  with that name, the new one is stored as `name (2).jar` instead of overwriting.
* **Remove** asks for confirmation, then moves the file to
  `%LOCALAPPDATA%\Kestrel\mods-trash\<UTC timestamp>-<name>`. **Recently removed**
  lists these with **Restore** (puts the mod back as disabled). Delete the trash folder
  by hand to free the space.
* **Kestrel client** (`kestrel-<version>.jar`) is built into `Kestrel.exe` and
  reinstalled on every Play, so it is shown as *Built in* without Disable/Remove.
  File names starting with `kestrel-` are reserved for it.
* Mod contents are only read (`mcmod.info` and the manifest, at most 1 MB each), never
  executed or extracted.

## Resource Packs

* **Detection.** Lists `instance\resourcepacks\*.zip` and folders, like Minecraft: a pack
  must have a valid `pack.mcmeta` at its top level (in the zip, or in the folder).
  Packs without one are shown greyed out with the reason and can't be enabled, because
  Minecraft would not list them either.
* **Preview.** Click a pack to see its icon (`pack.png`, max 1 MB and 512×512), name,
  description (from `pack.mcmeta`, formatting codes removed) and whether it was made
  for 1.8.9 (`pack_format` 1). Packs are read in place: only `pack.mcmeta` and
  `pack.png` are opened, with size limits; nothing is extracted or executed.
* **Enable / Disable / Up / Down.** The **Enabled** list is in priority order, top
  first, exactly like Minecraft's resource-pack menu: when two packs change the same
  texture, the higher one wins. A newly enabled pack goes on top, as in game.
* **Saved to `options.txt`** the way Minecraft 1.8.9 itself saves it (verified against
  the 1.8.9 client code):
  * `resourcePacks:["low.zip","high.zip"]` lists packs **lowest priority first**: the
    in-game menu reverses its on-screen list before saving, and the game searches
    packs from the last entry to the first.
  * A pack made for another version (`pack_format` ≠ 1) asks for confirmation, like
    the in-game menu, and is also written to `incompatibleResourcePacks`; without that
    entry Minecraft drops it at start-up.
  * Only those two lines are replaced; every other line of `options.txt` is kept byte
    for byte, and the file is replaced atomically. Pack names are written as plain
    ASCII JSON escapes so they read correctly in any Windows code page.
  * Your selection shows up in Minecraft's own Options → Resource Packs menu, and
    changes made there show up here.
* **Open Resource Pack Folder** opens `instance\resourcepacks` in Explorer (creating it
  if needed). Copy packs there, then click **Refresh**.

## Fullbright

A switch that lights up caves, nights and dark rooms as if everything were fully lit.
Turning it off restores Minecraft's normal lighting on the next frame.

How it works: Minecraft 1.8.9 computes its lightmap in `EntityRenderer.updateLightmap`,
which reads your Brightness (gamma) setting once. Kestrel's mixin
(`client/.../mixin/MixinEntityRenderer.java`) replaces only that read with
`max(your gamma, 16)` while Fullbright is on; with gamma ≥ 16 even the darkest lightmap
value (0.03) is pushed to full brightness. Your Brightness slider, `options.txt`, block
light, worlds and resource packs are never modified, so nothing can accumulate and
uninstalling Kestrel leaves no trace.

The setting is stored in the client config (`instance\kestrel\config.json` →
`"qol": {"fullbright": true}`). The launcher sends changes through the existing
launcher → client request file `instance\kestrel\launcher-cosmetics.json` (same
mechanism as the Cosmetics page: a strictly increasing `revision`, applied once,
atomically written, checked by the mod once per second). The QoL tab shows *Applying…* /
*Saved: applied the next time you press Play* until the client has applied it, then
*In sync with the client*.

## Files and folders

| Path (under `%LOCALAPPDATA%\Kestrel`) | Purpose |
|---|---|
| `instance\mods`, `instance\mods\1.8.9` | Active mods (Forge loads these) |
| `mods-disabled` | Disabled mods (moved back on Enable) |
| `mods-trash` | Removed mods, `<UTC timestamp>-<name>`; delete by hand |
| `instance\resourcepacks` | Resource packs |
| `instance\options.txt` | Minecraft settings; `resourcePacks` and `incompatibleResourcePacks` lines |
| `instance\kestrel\launcher-cosmetics.json` | Launcher → client request, now with `"qol": {"fullbright": bool}` (older launchers omit it; the client then keeps its value) |
| `instance\kestrel\config.json` | Client config, now with `"qol": {"fullbright": false}` by default |

Missing or malformed files never crash the launcher: a missing folder is an empty list,
an unreadable folder or `options.txt` line is shown as a message (the line is only
rewritten when you change the selection), and invalid JSON in the request/config files
falls back to defaults.

## Limitations

* Compatibility is a best-effort check from metadata; a mod that declares 1.8.9 can
  still conflict with another mod or need a library mod. If Minecraft crashes after
  adding a mod, disable it here.
* Only Forge 1.8.9 mods are supported. Fabric/Quilt/LiteLoader mods, mods for other
  versions and OptiFine-style installers are not.
* Downloading mods from the internet is intentionally not built in: install local files
  you trust.
* Resource packs and mods cannot be changed while the game runs (see above);
  Fullbright can.
