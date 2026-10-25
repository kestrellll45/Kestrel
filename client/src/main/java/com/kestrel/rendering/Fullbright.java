package com.kestrel.rendering;

/**
 * Fullbright: full lightmap brightness in caves and at night.
 *
 * <p>Minecraft 1.8.9 builds the lightmap in {@code EntityRenderer.updateLightmap}.
 * Each channel {@code c} (already floored to at least 0.03 and clamped to 1)
 * is mixed with its gamma curve: {@code c + g * ((1 - (1 - c)^4) - c)}, then
 * clamped to [0, 1]. With {@code g >= 16} even the darkest cell (0.03) reaches
 * 1.0, so every light level renders at full brightness.
 *
 * <p>{@link com.kestrel.mixin.MixinEntityRenderer} replaces only the value read
 * there by {@link #lightmapGamma(float)}. The player's real gamma setting
 * ({@code GameSettings.gammaSetting}, saved in options.txt) is never written,
 * so toggling cannot accumulate, cannot leak into options.txt, and turning it
 * off restores vanilla lighting on the next lightmap update. Block light, world
 * data and saves are untouched. Pure Java so it is unit-tested.
 */
public final class Fullbright {
    /** Lightmap gamma used while enabled (see class comment). */
    public static final float GAMMA = 16.0f;

    private static volatile boolean enabled;

    private Fullbright() {
    }

    public static boolean isEnabled() {
        return enabled;
    }

    public static void setEnabled(boolean on) {
        enabled = on;
    }

    /**
     * Gamma the lightmap should use, given the player's configured gamma.
     * Stateless: the configured value is passed in every frame and never stored.
     */
    public static float lightmapGamma(float configured) {
        if (!enabled) {
            return configured;
        }
        if (Float.isNaN(configured)) {
            return GAMMA;
        }
        return Math.max(configured, GAMMA);
    }
}
