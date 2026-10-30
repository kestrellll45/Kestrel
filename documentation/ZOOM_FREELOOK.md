# Zoom and free-look

Two hold-to-use camera features. Both are **camera-only**: your FOV setting, mouse
sensitivity (unless you opt in, below), movement and everything the server receives are
unchanged.

| Key (default) | Action |
|---|---|
| **C** (hold) | Zoom in smoothly; release to zoom back out. |
| **Left Alt** (hold) | Free-look: look around 360° while you keep moving the way you were facing; release to return. |

Rebind both under **Options → Controls → Kestrel** ("Zoom (hold)", "Free look (hold)");
they are saved in `options.txt` like any key binding. Mouse buttons work too. If you bind
one of them to a key that is also used by something else, both still work: Kestrel reads
the key's physical state, so it doesn't depend on Minecraft 1.8.9 delivering the key to only
one binding. The Controls screen still marks the clash in red so you notice it.

Settings (in-game menu, Right Shift → General → *Zoom & free look*):

| Setting | Default | Range |
|---|---|---|
| Zoom strength | 4.0x | 1.5x - 10x (FOV divided by this) |
| Smooth zoom | on | off = instant |
| Zoom speed | 14 | 2 - 40 (higher = faster; 14 is about 0.2 s, transition time = 3 / speed s) |
| Lower sensitivity while zoomed | off | scales mouse movement by the zoom |
| Free look in third person | on | switch to third person while held, back to first person afterwards |
| Smooth free-look return | on | ease back over 0.12 s instead of returning instantly |

## How it works

**Zoom** multiplies the world field of view for the current frame inside the hook that
already makes Kestrel's FOV static (`MixinEntityRenderer` → `StaticFov.fov(…, zoom)`). The
FOV setting is never written, so zoom can't stack, can't persist after release, and changing
the FOV slider while zoomed applies immediately. The held item and the debug camera are not
zoomed. Each press or release starts a timed transition from the current zoom, eased in and
out (smootherstep) and interpolated in log space, so the view magnifies at an even rate at
any FOV setting and zoom strength. Releasing mid-zoom reverses from where it is, without a jump.
It is frame-rate independent; a long frame (world load) continues the animation instead of
jumping to its end (`camera/ZoomState`).

**Free-look** gives the camera its own yaw and pitch (`camera/FreeLookState`):

- Mouse look: Minecraft 1.8.9 applies mouse movement through `Entity.setAngles`. While
  free-look is held, `MixinEntity` routes that movement to the free-look camera instead, so
  the player's rotation (and the rotation sent to the server) stays exactly where it was.
  No packets are created or changed.
- Camera: `MixinEntityRenderer` redirects the rotation reads in `orientCamera` (first person,
  third person and Forge's `CameraSetup` event all derive from them) to the camera angles,
  but only for the local player and only while free-look is active.
- So that the view stays consistent, the same angles are used for particle billboards
  (`MixinActiveRenderInfo`, vanilla's formula recomputed), name tags
  (`MixinRenderManager.cacheActiveRenderInfo`) and chunk visibility
  (`MixinRenderGlobal.setupTerrain`/`getViewVector`, so looking behind you never leaves
  chunks missing).
- The crosshair is hidden while free-looking, because it would not show where you are
  aiming. Attacking and using items still go where your body faces.
- Release returns the camera on the same frame. It eases back to where you are facing, the
  shortest way round, in at most 0.12 s whatever the angle, fastest at the start (ease-out),
  and ends exactly on your rotation; mouse movement turns your player again immediately. If
  Kestrel switched you to third person for free-look, it switches straight back to first person
  on release (that is already a cut, so there is no animation to wait for); it leaves the
  perspective alone if you pressed F5 meanwhile.

All hooks fall through to vanilla when the features are not in use. Each of these injectors
is declared with `require`, so a broken hook stops the game at start-up instead of silently
doing nothing.

## Using both together

Zoom and free-look are independent. Hold either or both. Releasing one while holding the other
releases only that one (zooming out, or returning the camera). With "Lower sensitivity while
zoomed", free-look movement is scaled by the zoom too.

## Safety and clean-up

Both keys are read only while you are actually playing: the game window focused, no screen
open (chat, inventory, pause, death screen) and the player alive. Losing focus (Alt-Tab), opening
any screen or dying therefore releases them; nothing can stay stuck. Changing world or dimension,
respawning and disconnecting reset both instantly.

Free-look is unavailable while sleeping, while spectating another entity and in the
developer debug camera.
