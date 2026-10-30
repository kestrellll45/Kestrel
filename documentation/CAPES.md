# Capes

## Where capes come from

| Source | Formats | Who sees it |
|---|---|---|
| Bundled catalog `client/src/main/resources/assets/kestrel/capes.json` | PNG (cape layout, optionally animated strips), GIF | Unrestricted capes: anyone can equip them; drawn on your own player by your client. Restricted capes: see below. |
| Custom folder `instance/kestrel/capes` (launcher **Upload cape…** or drop files in) | PNG ≤ 2 MB, GIF ≤ 4 MB | Your own player, your client only. |

The catalog is compiled into the client jar **and** into the launcher (`include_str!`), so
the two always agree. The mod reads it from its own jar (`Class#getResourceAsStream`),
not through Minecraft's resource manager, so resource packs cannot change it.

## Adding a cape

1. Put the texture in `client/src/main/resources/assets/kestrel/textures/capes/`.
2. Add an entry to `capes.json` (the comments below are explanations; JSON has none):

```json
{
  "id": "comet",                 // [a-z0-9_]{1,32}, unique, not "none"
  "name": "Comet (animated)",    // shown in the launcher and in game
  "texture": "capes/comet.gif",  // .png or .gif, relative to assets/kestrel/textures/
  "fallback": "capes/comet.png", // optional static PNG (cape layout) for GIFs
  "layout": "faces",             // GIFs only: "cape" (already a 2:1 cape sheet) or "faces"
  "scale": 8,                    // "faces": texture is 64*scale x 32*scale (1..16)
  "crop": [0, 46, 231, 406],     // "faces": optional source crop x, y, w, h (e.g. letterbox bars)
  "fill": "#FF8298",             // "faces": colour around the picture
  "edge": "#C9576D",             // "faces": colour of the cape's 1-texel edges
  "owners": ["<uuid>", "..."],   // optional: restrict to these Minecraft UUIDs
  "category": "animated",        // menu group: static (default), animated, special, kestrel
  "fps": 12                      // optional, PNG strips only (1-60): the cape's own frame rate;
                                 // without it the player's "Animation FPS" setting is used
}
```

3. Add a thumbnail `textures/capes/thumbs/<id>.png`: the outer face (10:16; for animated
   capes every frame's face side by side) and regenerate the launcher's embedded table
   `launcher/crates/kestrel-launcher/src/bundled_previews.rs` (`write_preview_table()` in
   `tools/gen_cosmetics.py`). The tests `every_catalog_entry_has_a_thumbnail` (Rust) and
   `bundledCatalogHasCategorisedStaticAndAnimatedCapes` (Java) fail until you do.
4. For a GIF with a `fallback`, regenerate the fallback from frame 0 (it must match exactly;
   `CapeCatalogTest.tetoFallbackMatchesTheFirstFrame` shows how: decode with `GifDecoder`,
   `CapeLayout.build(frame0, entry.layout)`, write PNG).
5. Run `scripts\test.ps1`.

Existing ids never change meaning, so saved selections (`ClientConfig.cosmetics.cape`)
keep working.

## Bundled designs

`tools/gen_cosmetics.py` generates the bundled capes from code (no external art): 24 static
designs (Kestrel Classic/Gold/Stealth, gradients, neon, geometric, elemental, pixel-art,
Galaxy, Obsidian Rune, Royal Crest, Carbon) at 4x (256x128) and 13 animated 24-frame
strips at 2x (128x64 per frame) with their own frame rate (RGB Cycle/Wave, Plasma, Tidal
Waves, Wildfire, Energy Core, Code Rain, Starfall, Pulse Rings, Kaleidoscope, Aurora Flow,
Lava Lamp, Kestrel Pulse). It also writes every thumbnail and the catalog. Menus group capes
by category (Static, Animated, Special, Kestrel, and Custom for your uploads), with a search
box; animated capes are marked ANIMATED and their thumbnails play.

## Layouts

The vanilla cape is a 10×16×1 box textured from a 64×32 sheet (× `scale` for HD): outer face
(seen from behind the player) at (1,1), inner face (towards the player's back) at (12,1),
1-texel edges around them. `"faces"` scales the picture to fit 10:16 (aspect kept, padded
with `fill`) and draws it **unmirrored on both faces**, so it reads correctly from either
side. A 2:1 GIF whose width is a multiple of 64 is used as a cape sheet directly (`"cape"`).

## GIF animation

`GifDecoder` uses the JDK's GIF reader only for LZW decoding and does compositing itself:
frame offsets, disposal (none / keep / restore-to-background (transparent) /
restore-to-previous), transparency and per-frame delays (0–1 cs → 100 ms, as browsers do).
The Teto GIF's 24 composited frames match Pillow's output bit for bit.

Limits (identical in `kestrel-core/src/cosmetics.rs`): file ≤ 4 MB, logical screen and every
frame rectangle ≤ 1024 px per side (checked before any pixel is decoded), ≤ 128 frames,
≤ 12 M decoded texels per cape. Malformed files produce a user-facing error, never a crash.

Runtime: decoding runs on the loader thread; frames are uploaded once (≤ 4 per tick) as
textures and cached per cape id, shared by every player wearing that cape. Nothing is
allocated per frame; the current frame is picked by `FrameTimeline` from the wall clock.
"Animate capes" off shows frame 0. If a bundled GIF cannot be decoded, its `fallback` PNG is
used. Textures of capes nobody wears are freed (other players' capes 30 s after they were
last drawn).

## Restricted capes (`owners`)

* Identity is the **UUID only**, normalised (case- and dash-insensitive). Usernames are
  never consulted, so another account named `bringing` does not get the cape, and an
  offline session (UUID derived from `OfflinePlayer:<name>`) never matches.
* An `owners` key with no valid UUID restricts the cape to nobody (fail closed); an invalid
  UUID makes the catalog fail its tests.
* **Your own player:** `CapeManager.localCapeId()` refuses a restricted cape unless the
  session UUID (`Minecraft.getSession().getPlayerID()`, from Microsoft sign-in) is an owner.
  This check runs at render time, so editing `config.json` or the launcher request file does
  not bypass it. The in-game list and the launcher only offer it to the owner.
* **Other players:** Kestrel clients draw a restricted cape on the player whose UUID owns it
  (the UUID the server reports, authenticated by Mojang on online-mode servers). Players can
  hide their cape with Skin Customization → Cape, and viewers can turn this off with
  "Show players' assigned capes". An official Minecraft cape always takes precedence.

Limitations, honestly: there is no Kestrel server. The restriction is enforced by the
signed client jar; anyone who modifies their own client could render any texture locally
(only on their own screen). Other viewers see the assigned cape only if they run Kestrel,
and they see it whenever the owner is visible, because the owner's in-game cape choice is
not transmitted. Nicked or offline-mode players are identified by whatever UUID the server
gives them.

## Kasane Teto cape

`teto` uses `capes/teto.gif` (231×498, 24 frames, supplied by the project owner) with the
letterbox bars cropped (`crop [0, 46, 231, 406]`), fitted on both faces at scale 8
(512×256 texture, ≈ 12 MB of textures for all frames), static fallback `capes/teto.png`.
It is restricted to UUID `ba14933e-a7ad-4816-8696-a788cc1b3c1f`.

## Lunar Client cosmetics

Researched October 2026:

1. **Officially documented:** Lunar Client's public developer API is *Apollo*
   (github.com/LunarClient/Apollo, lunarclient.dev), a **server-side** API that lets
   server plugins interact with players who use Lunar Client. It offers nothing for a
   third-party client to fetch, display or own Lunar cosmetics.
2. **Undocumented:** Lunar's cosmetics come from its private, authenticated services.
   Using them would mean impersonating Lunar Client or reusing its sessions, against its
   terms and against Kestrel's rules (no token reuse, no entitlement bypass), so Kestrel
   does not do it, and does not read Lunar's local files.
3. **What Kestrel supports instead:** your own assets in its own renderer: PNG capes
   (vanilla/HD/animated strips) and GIF capes, via **Upload cape…** or the capes folder.
   This is not a Lunar integration, and the UI does not call it one. If Lunar ever
   publishes a client-side interoperability API, it can be added next to the catalog as a
   separate cape source.
