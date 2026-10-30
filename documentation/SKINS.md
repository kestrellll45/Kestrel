# Local skin changer

Wear any skin in your own Kestrel client without changing your Minecraft account.

**This is a local, client-side appearance only.** Kestrel does not upload anything to
Mojang or Microsoft, does not change your account's skin, does not touch Microsoft sign-in,
and sends nothing to servers. Other players keep seeing your real account skin. With no
custom skin selected, your account skin is used as normal.

## Where

- **Launcher → Skin**: a front/back preview, *Import Skin…*, *Load from URL* (NameMC),
  *Apply Skin*, *Restore Account Skin*, classic/slim arms, your skin library, status and
  errors.
- **In game → main menu → Skin** (button to the right of *Quit Game*): the same features with
  a rotatable 3D preview (drag to rotate). It works with the launcher closed. *Import PNG…*
  opens the system file picker (it can open behind the game window in fullscreen). If no
  picker is available, *Open folder* opens `skins\import`: put PNGs there and press
  *Refresh*.

Both use the same files, so they stay in sync. A skin applied in the launcher shows up in a
running game within about a second; changes made in game appear in the launcher's Skin page.

The custom skin is used for your player in third person, the inventory preview, your
first-person arm (with the right arm width) and your tab-list head. Capes, animated capes and
other cosmetics are unaffected.

## Skin files

- PNG only, **64x64** or the old **64x32** format, up to 512 KB. Anything else is rejected
  with a message, before anything is written.
- The file is copied unchanged (transparency included) into the library. Like Minecraft
  itself, the game shows the base layer opaque, and old 64x32 skins get mirrored left
  arms and legs.
- **Classic** (4 px) or **slim** (3 px) arms. Kestrel suggests one by looking at the arm
  pixels; you can change it. Old 64x32 skins are always classic.

## NameMC

- **Profile links work:** `https://namemc.com/profile/<name>` (also `…/profile/<name>.1`,
  `…/profile/<uuid>`, and NameMC's language subdomains). Kestrel loads that player's
  current skin and arm model through Mojang's public profile API (`api.mojang.com` →
  `sessionserver.mojang.com` → `textures.minecraft.net`). Direct
  `textures.minecraft.net/texture/…` links work too.
- **Skin pages don't:** `https://namemc.com/skin/<id>` can't be loaded automatically.
  NameMC has no public API for skin pages and blocks automated requests, and Kestrel does
  not scrape or guess undocumented endpoints. Open the page in your browser, click
  **Download**, then import the PNG. Kestrel tells you this when you paste such a link.

Every other site is refused before any network request. Requests go only to the three
Mojang hosts above, without cookies or credentials, without following redirects, with
10-second timeouts and size limits (64 KB for profile data, 512 KB for a skin). The game
fetches skin textures over https; if that fails it falls back to plain http for
`textures.minecraft.net` only, which is how Minecraft 1.8.9 itself downloads every skin.
The downloaded image is validated like any imported file.

## Storage

`%LOCALAPPDATA%\Kestrel\instance\kestrel\skins\` (or `<root>\instance\kestrel\skins\` with
`--root`):

| File | Contents |
|---|---|
| `<16 hex>.png` | validated copies of your skins, named by content hash (re-importing never duplicates) |
| `skin.json` | the library (name, arms, source) and the active skin; `"active": null` = account skin |
| `import\` | optional drop folder for the in-game *Refresh* button; files there are never modified |

`skin.json` is written atomically (temp file + rename) by both the launcher and the game,
each with an increasing `revision`. File names from it are accepted only in the
`<16 hex>.png` form, so a hand-edited file can't point outside the folder. A missing or
malformed file just means "account skin". *Remove* deletes Kestrel's copy only, never the
file you imported from.

## Code

| Part | Files |
|---|---|
| Shared rules (Rust) | `launcher/crates/kestrel-core/src/skins.rs` (state, validation, preview, URL parsing/resolution) |
| Launcher page | `launcher/crates/kestrel-launcher/src/skin_page.rs` |
| Game | `client/.../skin/SkinStore.java`, `SkinUrl.java` (same rules in Java), `SkinManager.java` (texture, polling), `SkinScreen.java`, `MainMenuButton.java`; `mixin/MixinAbstractClientPlayer`, `MixinNetworkPlayerInfo` |

The game matches "you" by your profile UUID (the session's, or the player entity's on
offline-mode servers), so no other player's skin can change.
