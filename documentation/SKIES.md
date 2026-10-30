# Custom skies

Put PNG files in `%LOCALAPPDATA%\Kestrel\instance\kestrel\skies\` (Kestrel menu → Sky →
"Open custom skies folder"), then pick them under **Sky**.

## Format

- PNG, 3:2 aspect ratio, 384–3072 px wide, at most 16 MB.
- A 3×2 grid of square faces:

```
+--------+--------+--------+
| north  |  east  | south  |
+--------+--------+--------+
|  west  |   up   |  down  |
+--------+--------+--------+
```

- Side faces (north, east, south, west) are drawn as seen from inside the sky,
  standing upright: image top = sky, image bottom = ground, left/right as you see it
  when facing that direction. A horizon drawn across the middle of all four lines up.
- **up**: image top edge borders south (+Z), left edge borders west (−X).
- **down**: image top edge borders north (−Z), left edge borders west (−X).

The exact corner table is in `client/src/main/java/com/kestrel/rendering/SkyboxRenderer.java`.
`tools/gen_assets.py` uses the same table, so you can adapt it to bake your own seamless
skies from any function of view direction.

## Bundled skies

`client/src/main/resources/assets/kestrel/skies.json` lists the bundled skies (id, name,
category); atlases are `textures/sky/<id>.png` and menu thumbnails (the north/east/south
strip, 384x128) `textures/sky/thumbs/<id>.png`. The launcher embeds the same file. Categories:
Sunsets, Space, Clouds & Aurora, Dark, Neon, Fantasy, Minimal, Blocky and Kestrel; uploads are
listed under Custom, and "Vanilla sky" turns the custom sky off. Switching applies
immediately in game (no restart). `tools/gen_cosmetics.py` generates the bundled skies
(seam-free, same corner table) and their thumbnails.

Custom skies apply in the Overworld only and replace the sun, moon and stars while enabled.
If another mod installs its own sky renderer, Kestrel leaves it alone and reports it.
