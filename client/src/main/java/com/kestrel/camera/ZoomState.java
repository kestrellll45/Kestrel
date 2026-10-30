package com.kestrel.camera;

/**
 * Hold-to-zoom animation state. Pure (no Minecraft types) so it is unit-tested.
 *
 * <p>The state is a single multiplier applied to the world field of view for
 * the current frame only ({@code 1} = no zoom). It never touches
 * {@code GameSettings.fovSetting}, so the FOV setting is never changed and
 * zooming can never stack.
 *
 * <p>Each change of target (key pressed or released) starts a timed
 * transition from the current multiplier, eased in and out (smootherstep), so
 * it starts and stops without a jolt. The interpolation runs in log space, so
 * the field of view changes by the same ratio per moment of time: equal steps
 * of perceived magnification, at any base FOV and zoom strength.
 */
public final class ZoomState {
    /**
     * Longest frame time used for animation. A hitch (world load, GC) then
     * continues the animation instead of jumping to its end.
     */
    static final float MAX_DT = 0.05f;
    /** Transition duration bounds in seconds. */
    static final float MIN_DURATION = 0.06f;
    static final float MAX_DURATION = 1.5f;

    private float multiplier = 1.0f;
    private float from = 1.0f;
    private float to = 1.0f;
    /** Progress of the current transition in [0, 1]; 1 = finished. */
    private float progress = 1.0f;

    /**
     * Advance one frame.
     *
     * @param held   whether the zoom key is held (and zoom is allowed)
     * @param factor how far to zoom in (the FOV is divided by this), &gt;= 1
     * @param speed  the "Zoom speed" setting (higher = faster), see {@link #duration}
     * @param smooth false switches instantly
     * @param dt     seconds since the previous frame
     */
    public void update(boolean held, float factor, float speed, boolean smooth, float dt) {
        float target = held ? 1.0f / sanitizeFactor(factor) : 1.0f;
        if (!smooth || !(speed > 0f)) {
            multiplier = from = to = target;
            progress = 1.0f;
            return;
        }
        if (target != to) {
            // Retarget from wherever we are (also mid-transition): no jump.
            from = multiplier;
            to = target;
            progress = 0.0f;
        }
        if (progress >= 1.0f || !(dt > 0f)) {
            return;
        }
        progress = Math.min(1.0f, progress + Math.min(dt, MAX_DT) / duration(speed));
        if (progress >= 1.0f) {
            multiplier = to;
        } else {
            double e = smootherstep(progress);
            multiplier = (float) Math.exp(Math.log(from) + (Math.log(to) - Math.log(from)) * e);
        }
    }

    /** Transition time in seconds for a "Zoom speed" value (14 = about 0.21 s). */
    static float duration(float speed) {
        float d = 3.0f / speed;
        return d < MIN_DURATION ? MIN_DURATION : (d > MAX_DURATION ? MAX_DURATION : d);
    }

    /** 6t^5 - 15t^4 + 10t^3: zero velocity and acceleration at both ends. */
    static double smootherstep(float t) {
        double x = t <= 0f ? 0.0 : (t >= 1f ? 1.0 : t);
        return x * x * x * (x * (x * 6.0 - 15.0) + 10.0);
    }

    /** Multiplier for the world FOV this frame, in (0, 1]. */
    public float fovMultiplier() {
        return multiplier;
    }

    public boolean isZoomed() {
        return multiplier < 1.0f;
    }

    /** True while a transition is running. */
    public boolean isAnimating() {
        return progress < 1.0f;
    }

    /** Back to no zoom immediately (world change, disconnect, respawn). */
    public void reset() {
        multiplier = from = to = 1.0f;
        progress = 1.0f;
    }

    static float sanitizeFactor(float factor) {
        if (Float.isNaN(factor) || factor < 1.0f) return 1.0f;
        return Math.min(factor, 50.0f);
    }
}
