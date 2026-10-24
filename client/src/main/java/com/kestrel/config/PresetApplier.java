package com.kestrel.config;

/**
 * Applies a {@link Preset} to the client config.
 *
 * <p>Presets only touch settings that affect cost or appearance (sky, cape
 * animation, kill-effect richness, HUD refresh rate, and in Competitive the
 * information modules the preset is defined to show). HUD positions, scales,
 * colours, keybinds, crosshair, selected cape and sky are never changed, so
 * switching presets does not erase unrelated preferences. Everything remains
 * individually adjustable afterwards.
 *
 * <p>This class is pure (no Minecraft types) so it is unit-tested; vanilla
 * video settings are handled separately by the PerformanceManager and only
 * when the user opts in.
 */
public final class PresetApplier {
    private PresetApplier() {
    }

    public static void apply(ClientConfig c, Preset p) {
        switch (p) {
            case POTATO:
                c.sky.enabled = false;
                c.cosmetics.animatedCapes = false;
                c.killEffects.effect = "burst";
                c.killEffects.intensity = 0.5f;
                c.killEffects.maxParticles = 15;
                c.performance.hudRefreshMs = 250;
                break;
            case COMPETITIVE:
                enableHud(c, "fps");
                enableHud(c, "cps");
                enableHud(c, "keystrokes");
                c.cosmetics.animatedCapes = false;
                c.killEffects.intensity = Math.min(c.killEffects.intensity, 1.0f);
                c.killEffects.maxParticles = Math.min(c.killEffects.maxParticles, 40);
                if ("lightning".equals(c.killEffects.effect) || "explosion".equals(c.killEffects.effect)) {
                    c.killEffects.effect = "burst";
                }
                c.performance.hudRefreshMs = 100;
                break;
            case VISUAL:
                c.sky.enabled = true;
                c.cosmetics.animatedCapes = true;
                c.killEffects.intensity = 1.5f;
                c.killEffects.maxParticles = 150;
                c.performance.hudRefreshMs = 100;
                break;
            default:
                throw new IllegalArgumentException("unknown preset " + p);
        }
        c.activePreset = p.id;
        c.validate();
    }

    /**
     * Apply the launcher-requested preset once: only when it differs from the
     * last launcher request we honoured, so in-game changes are not reset on
     * every launch. Returns the applied preset, or null.
     */
    public static Preset applyLauncherRequest(ClientConfig c, String requested) {
        Preset p = Preset.byId(requested);
        if (p == null || p.id.equals(c.lastLauncherPreset)) {
            return null;
        }
        apply(c, p);
        c.lastLauncherPreset = p.id;
        return p;
    }

    private static void enableHud(ClientConfig c, String id) {
        // HUD elements register their default configs before any preset is
        // applied, so the entry normally exists; the fallback keeps this
        // method total for tests and future callers.
        ClientConfig.HudElementConfig e = c.hud.get(id);
        if (e == null) {
            e = new ClientConfig.HudElementConfig();
            c.hud.put(id, e);
        }
        e.enabled = true;
    }
}
