package com.kestrel.camera;

/**
 * 360-degree free-look camera angles, separate from the player's rotation.
 * Pure (no Minecraft types) so it is unit-tested.
 *
 * <p>While the key is held, mouse movement turns these angles instead of the
 * player (same maths as {@code Entity.setAngles}). The player's own
 * rotation, and therefore movement and everything sent to the server, is
 * left exactly as it was.
 *
 * <p>On release the camera returns to wherever the player is facing (shortest
 * way round). The return starts on the same frame and takes a fixed
 * {@link #RETURN_DURATION} at most, whatever the angle, eased out so it moves
 * fastest at the start and settles without a snap. It ends exactly on the
 * player's rotation.
 */
public final class FreeLookState {
    /** Longest frame time used for the return animation. */
    static final float MAX_DT = 0.05f;
    /** Duration of the smooth return, in seconds. */
    public static final float RETURN_DURATION = 0.12f;

    private boolean held;
    private boolean returning;
    private float yaw;
    private float pitch;
    private float fromYaw;
    private float fromPitch;
    private float progress;

    /** Key pressed: start from the player's view (or continue from a return). */
    public void press(float playerYaw, float playerPitch) {
        if (!held && !returning) {
            yaw = playerYaw;
            pitch = clampPitch(playerPitch);
        }
        held = true;
        returning = false;
    }

    /**
     * Key released (or focus lost). With {@code smoothReturn} the camera eases
     * back over {@link #RETURN_DURATION}; otherwise it is back at once.
     */
    public void release(boolean smoothReturn) {
        if (!held) return;
        held = false;
        returning = smoothReturn;
        fromYaw = yaw;
        fromPitch = pitch;
        progress = 0f;
    }

    /** Mouse movement while held; same scale as {@code Entity.setAngles}. */
    public void turn(float dx, float dy) {
        yaw = (float) (yaw + dx * 0.15);
        pitch = clampPitch((float) (pitch - dy * 0.15));
    }

    /**
     * Advance the return towards the player's current rotation (which may
     * itself move: mouse look goes to the player again as soon as the key is
     * released).
     */
    public void update(float dt, float playerYaw, float playerPitch, float duration) {
        if (!returning) return;
        if (dt > 0f) {
            float d = duration > 0f ? duration : RETURN_DURATION;
            progress = Math.min(1f, progress + Math.min(dt, MAX_DT) / d);
        }
        if (progress >= 1f) {
            yaw = playerYaw;
            pitch = clampPitch(playerPitch);
            returning = false;
            return;
        }
        double e = easeOutCubic(progress);
        yaw = (float) (fromYaw + wrapDegrees(playerYaw - fromYaw) * e);
        pitch = (float) (fromPitch + (clampPitch(playerPitch) - fromPitch) * e);
    }

    /** True while the camera must use these angles instead of the player's. */
    public boolean overrides() {
        return held || returning;
    }

    public boolean isHeld() {
        return held;
    }

    public boolean isReturning() {
        return returning;
    }

    public float yaw() {
        return yaw;
    }

    public float pitch() {
        return pitch;
    }

    /** Stop immediately (world change, disconnect, death screen). */
    public void reset() {
        held = false;
        returning = false;
    }

    static double easeOutCubic(float t) {
        double x = 1.0 - (t <= 0f ? 0.0 : (t >= 1f ? 1.0 : t));
        return 1.0 - x * x * x;
    }

    static float clampPitch(float p) {
        if (Float.isNaN(p)) return 0f;
        return p < -90f ? -90f : (p > 90f ? 90f : p);
    }

    /** Wrap to [-180, 180). */
    static float wrapDegrees(float d) {
        d %= 360f;
        if (d >= 180f) d -= 360f;
        if (d < -180f) d += 360f;
        return d;
    }
}
