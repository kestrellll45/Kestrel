package com.kestrel.config;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * In-game client settings, persisted as JSON in {@code <gameDir>/kestrel/config.json}.
 *
 * <p>Every field has a default so that missing keys (older files, partial
 * edits) keep sensible values; {@link #validate()} clamps everything after
 * loading. All classes keep an implicit no-arg constructor so Gson applies the
 * field initialisers.
 */
public final class ClientConfig {
    public static final int CURRENT_VERSION = 1;

    public int configVersion = CURRENT_VERSION;
    /** Preset most recently applied, for display only. */
    public String activePreset = Preset.COMPETITIVE.id;
    /** Last preset requested by the launcher that we applied (see PresetApplier). */
    public String lastLauncherPreset = "";
    /** Revision of the last launcher cosmetics request applied (see LauncherSync). */
    public long lastLauncherCosmeticsRevision = 0;

    public Map<String, HudElementConfig> hud = new LinkedHashMap<String, HudElementConfig>();
    public Crosshair crosshair = new Crosshair();
    public Sprint sprint = new Sprint();
    public Cosmetics cosmetics = new Cosmetics();
    public Sky sky = new Sky();
    public KillEffects killEffects = new KillEffects();
    public Performance performance = new Performance();
    public Qol qol = new Qol();
    public Zoom zoom = new Zoom();
    public FreeLook freeLook = new FreeLook();

    public static final class HudElementConfig {
        public boolean enabled = false;
        /** Top-left position as a fraction of the scaled screen size (0..1). */
        public float x = 0.01f;
        public float y = 0.01f;
        public float scale = 1.0f;
        public boolean background = true;
        public int textColor = 0xFFFFFFFF;
        public boolean textShadow = true;

        public HudElementConfig() {
        }

        public HudElementConfig(boolean enabled, float x, float y) {
            this.enabled = enabled;
            this.x = x;
            this.y = y;
        }

        void validate() {
            x = clamp(x, 0f, 1f);
            y = clamp(y, 0f, 1f);
            scale = clamp(scale, 0.5f, 3.0f);
            textColor |= 0xFF000000;
        }
    }

    public static final class Crosshair {
        public boolean custom = false;
        /** 0 = cross, 1 = dot, 2 = cross with dot, 3 = T-shape. */
        public int style = 0;
        public int length = 5;
        public int thickness = 2;
        public int gap = 3;
        public int color = 0xFFFFFFFF;
        public boolean outline = true;
        public int outlineColor = 0xFF000000;
        public boolean hideInThirdPerson = true;

        void validate() {
            style = clampi(style, 0, 3);
            length = clampi(length, 1, 20);
            thickness = clampi(thickness, 1, 8);
            gap = clampi(gap, 0, 15);
            color |= 0xFF000000;
            outlineColor |= 0xFF000000;
        }
    }

    public static final class Sprint {
        public boolean toggleSprint = true;
        /** Persisted toggle state so sprint stays on across sessions. */
        public boolean toggled = false;
    }

    public static final class Cosmetics {
        public boolean capesEnabled = true;
        /** Cape id: "none", a catalog id (capes.json), or "custom:<file name>" (PNG or GIF). */
        public String cape = "none";
        public boolean animatedCapes = true;
        /** Draw capes the bundled catalog assigns to specific players (by UUID) on those players. */
        public boolean showAssignedCapes = true;
        /** Frames per second for animated capes. */
        public int animationFps = 10;

        void validate() {
            if (cape == null || cape.isEmpty()) {
                cape = "none";
            }
            animationFps = clampi(animationFps, 1, 30);
        }
    }

    public static final class Sky {
        public boolean enabled = false;
        /** Sky id: bundled preset id or "custom:<file name>". */
        public String preset = "dusk";
        public float brightness = 1.0f;
        public boolean rotate = true;
        /** Degrees per second when rotating. */
        public float rotationSpeed = 0.5f;
        /** Dim the custom sky at night using the world's sun angle. */
        public boolean followDayCycle = false;

        void validate() {
            if (preset == null || preset.isEmpty()) {
                preset = "dusk";
            }
            brightness = clamp(brightness, 0.1f, 1.0f);
            rotationSpeed = clamp(rotationSpeed, 0f, 10f);
        }
    }

    public static final class KillEffects {
        public boolean enabled = true;
        /** {@link com.kestrel.effects.KillEffectType} id. */
        public String effect = "burst";
        public float intensity = 1.0f;
        /** Upper bound on particles a single effect may spawn. */
        public int maxParticles = 60;
        public boolean sounds = true;
        public float volume = 0.7f;
        /** Colour used by the colour effects (helix, colour burst). Opaque ARGB. */
        public int color = 0xFFE8833A;
        /** Credit kills from the server's death message when it makes you the killer (KillMessageMatcher). */
        public boolean chatDetection = true;
        /** Credit kills when a player dies right after your hit with no other damage in between (KillAttribution). */
        public boolean healthDetection = true;

        void validate() {
            if (effect == null || effect.isEmpty()) {
                effect = "burst";
            }
            intensity = clamp(intensity, 0.25f, 2.0f);
            maxParticles = clampi(maxParticles, 0, 200);
            volume = clamp(volume, 0f, 1f);
            color |= 0xFF000000;
        }
    }

    public static final class Performance {
        /** When applying a preset, also change vanilla video settings. Off by default. */
        public boolean applyVanillaSettings = false;
        /** How often HUD text is rebuilt, in milliseconds. */
        public int hudRefreshMs = 100;

        void validate() {
            hudRefreshMs = clampi(hudRefreshMs, 16, 1000);
        }
    }

    /** Quality-of-life features toggled from the launcher's QoL page. */
    public static final class Qol {
        /** Full lightmap brightness (see {@link com.kestrel.rendering.Fullbright}). */
        public boolean fullbright = false;
    }

    /** Hold-to-zoom (default key C; see {@link com.kestrel.camera.CameraController}). */
    public static final class Zoom {
        /** How far to zoom in: the field of view is divided by this. */
        public float factor = 4.0f;
        /** Transition speed (exponential rate per second; higher is faster). */
        public float speed = 14.0f;
        /** Animate zooming in and out; off switches instantly. */
        public boolean smooth = true;
        /** Lower mouse sensitivity while zoomed, in proportion to the zoom. */
        public boolean scaleSensitivity = false;

        void validate() {
            factor = clamp(factor, 1.5f, 10.0f);
            speed = clamp(speed, 2.0f, 40.0f);
        }
    }

    /** 360-degree free-look (default key Left Alt). */
    public static final class FreeLook {
        /** Switch to third person while held (back to first person afterwards). */
        public boolean thirdPerson = true;
        /** Ease the camera back on release instead of returning instantly. */
        public boolean smoothReturn = true;
    }

    /** Clamp every value into a safe range and repair nulls. */
    public void validate() {
        if (hud == null) {
            hud = new LinkedHashMap<String, HudElementConfig>();
        }
        for (Map.Entry<String, HudElementConfig> e : hud.entrySet()) {
            if (e.getValue() == null) {
                e.setValue(new HudElementConfig());
            }
            e.getValue().validate();
        }
        if (crosshair == null) crosshair = new Crosshair();
        if (sprint == null) sprint = new Sprint();
        if (cosmetics == null) cosmetics = new Cosmetics();
        if (sky == null) sky = new Sky();
        if (killEffects == null) killEffects = new KillEffects();
        if (performance == null) performance = new Performance();
        if (qol == null) qol = new Qol();
        if (zoom == null) zoom = new Zoom();
        if (freeLook == null) freeLook = new FreeLook();
        if (Preset.byId(activePreset) == null) activePreset = Preset.COMPETITIVE.id;
        if (lastLauncherPreset == null) lastLauncherPreset = "";
        crosshair.validate();
        cosmetics.validate();
        sky.validate();
        killEffects.validate();
        performance.validate();
        zoom.validate();
    }

    /** Returns the element config, creating it with the given defaults if absent. */
    public HudElementConfig hudElement(String id, boolean enabledByDefault, float x, float y) {
        HudElementConfig c = hud.get(id);
        if (c == null) {
            c = new HudElementConfig(enabledByDefault, x, y);
            hud.put(id, c);
        }
        return c;
    }

    static float clamp(float v, float lo, float hi) {
        if (Float.isNaN(v)) return lo;
        return v < lo ? lo : (v > hi ? hi : v);
    }

    static int clampi(int v, int lo, int hi) {
        return v < lo ? lo : (v > hi ? hi : v);
    }
}
