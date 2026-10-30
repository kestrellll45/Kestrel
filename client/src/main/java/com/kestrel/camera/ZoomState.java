package com.kestrel.camera;

/**
 * Hold-to-zoom animation state. Pure (no Minecraft types) so it is unit-tested.
 *
 * <p>The state is a single multiplier applied to the world field of view for
 * the current frame only ({@code 1} = no zoom). It never touches
 * {@code GameSettings.fovSetting}, so the FOV setting is never changed and
 * zooming can never stack.
 */
public final class ZoomState {
    /** Longest frame time used for animation; longer stalls don't jump. */
    static final float MAX_DT = 0.25f;
    /** Below this distance from the target the animation snaps and ends. */
    static final float EPSILON = 1.0e-4f;

    private float multiplier = 1.0f;

    /**
     * Advance one frame.
     *
     * @param held   whether the zoom key is held (and zoom is allowed)
     * @param factor how far to zoom in (the FOV is divided by this), &gt;= 1
     * @param speed  exponential approach rate per second (higher = faster)
     * @param smooth false switches instantly
     * @param dt     seconds since the previous frame
     */
    public void update(boolean held, float factor, float speed, boolean smooth, float dt) {
        float target = held ? 1.0f / sanitizeFactor(factor) : 1.0f;
        if (!smooth || !(speed > 0f)) {
            multiplier = target;
            return;
        }
        if (!(dt > 0f)) {
            return;
        }
        float k = (float) (1.0 - Math.exp(-speed * Math.min(dt, MAX_DT)));
        multiplier += (target - multiplier) * k;
        if (Math.abs(target - multiplier) < EPSILON) {
            multiplier = target;
        }
    }

    /** Multiplier for the world FOV this frame, in (0, 1]. */
    public float fovMultiplier() {
        return multiplier;
    }

    public boolean isZoomed() {
        return multiplier < 1.0f;
    }

    /** Back to no zoom immediately (world change, disconnect, respawn). */
    public void reset() {
        multiplier = 1.0f;
    }

    static float sanitizeFactor(float factor) {
        if (Float.isNaN(factor) || factor < 1.0f) return 1.0f;
        return Math.min(factor, 50.0f);
    }
}
