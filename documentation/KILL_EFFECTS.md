# Kill effects

A kill effect plays at the victim's position when **you** kill another **player**. It is
drawn by your client only (particles, sounds and some custom geometry); nothing is sent to
the server.

## When an effect plays

Only for a player you hit yourself, and only when the kill is clearly yours:

- **Never** for mobs or other non-player entities, other players' kills, deaths from
  falling, lava, fire, the void or mobs, or your own death.
- **Death seen in the world** (`healthDetection`): the victim's health reaches 0, the last
  damage it took arrived within 1 s of your hit, it died within 1.5 s of your hit, and no
  other player was swinging within 6 blocks of it (or an arrow that is not yours flying at
  it) at any time from 1 s before your last hit until its death, as far as your client
  saw. Implemented in `KillAttribution` (unit-tested).
- **Kill-feed message** (`chatDetection`): a server line that names you and the victim *and*
  makes you the killer: "<victim> was slain by <you>", "<you> killed <victim>". Lines such as
  "<you> was slain by <victim>", "... assisted by <you>" and player chat never match.
  Implemented in `KillMessageMatcher` (unit-tested).
- If attribution is unclear, nothing plays. A kill confirmed both ways plays once; the same
  victim cannot trigger another effect for 6 s.

## Effects

| Effect | Category | Colour setting | Sound | What it does |
|---|---|---|---|---|
| Particle burst (`burst`) | Classic | - | `random.orb` |  |
| Particle ring (`ring`) | Classic | - | `random.orb` |  |
| Colour helix (`helix`) | Classic | yes | `random.orb` |  |
| Colour burst (`dust`) | Classic | yes | `random.orb` |  |
| Explosion (`explosion`) | Classic | - | `random.explode` |  |
| Smoke bomb (`smoke`) | Classic | - | `random.fizz` |  |
| Pixel blast (`pixel`) | Classic | - | `random.pop` |  |
| Lightning (`lightning`) | Elemental | - | `ambient.weather.thunder` |  |
| Flame pillar (`flames`) | Elemental | - | `mob.ghast.fireball` |  |
| Electric shock (`shock`) | Elemental | - | `random.fizz` |  |
| Ice shatter (`ice`) | Elemental | - | `dig.glass` |  |
| Nether blaze (`nether`) | Elemental | - | `mob.blaze.hit` |  |
| Tornado (`tornado`) | Elemental | - | `mob.enderdragon.wings` |  |
| Arcane (`arcane`) | Magic | - | `mob.endermen.portal` |  |
| Soul release (`soul`) | Magic | - | `mob.ghast.moan` |  |
| Ender vanish (`ender`) | Magic | - | `mob.endermen.portal` |  |
| Portal (`portal`) | Magic | - | `mob.endermen.portal` |  |
| Dragon breath (`dragon`) | Magic | - | `mob.enderdragon.growl` |  |
| Energy burst (`energy`) | Magic | yes | `random.orb` |  |
| Hearts (`hearts`) | Celebration | - | `random.levelup` |  |
| Fireworks (`fireworks`) | Celebration | yes | `fireworks.blast` |  |
| Confetti (`confetti`) | Celebration | - | `fireworks.twinkle` |  |
| Rainbow rings (`rainbow`) | Celebration | - | `random.orb` |  |
| Falling stars (`starfall`) | Celebration | - | `fireworks.twinkle` |  |
| Kestrel strike (`kestrel`) | Kestrel | - | `mob.bat.takeoff` |  |

Effects tinted by the colour setting are marked COLOR in game and COLOUR in the launcher.
Particles are capped per effect ("Max particles per effect") and at most a few effects run
at once, so FPS stays reasonable; every effect removes itself when it finishes, or when
you leave the world.

## Previewing

In game: Right Shift → Kill effects → click an effect (equips it and plays it in front of
you), or **Preview effect**. The launcher's Cosmetics → Kill effects page shows an
approximate 2D animation of each effect.
