package com.kestrel.camera;

/**
 * 360-degree free-look camera angles, separate from the player's rotation.
 * Pure (no Minecraft types) so it is unit-tested.
 *
 * <p>While the key is held, mouse movement turns these angles instead of the
 * player (same maths as {@code Entity.setAngles}). The player's own
 * rotation, and therefore movement and everything sent to the server, is
 * left exactly as it was. On release the camera eases back to wherever the
 * player is facing (shortest way round) instead of snapping.
 */
public final class FreeLookState {
    static final float MAX_DT = 0.25f;
    /** Angle (degrees) at which the return animation finishes. */
    static final float DONE = 0.05f;
    /** Speed of the return animation (exponential rate per second). */
    public static final float RETURN_SPEED = 14.0f;

    private boolean held;
    private boolean returning;
    private float yaw;
    private float pitch;

    /** Key pressed: start from the player's view (or continue a return). */
    public void press(float playerYaw, float playerPitch) {
        if (!held && !returning) {
            yaw = playerYaw;
            pitch = clampPitch(playerPitch);
        }
        held = true;
        returning = false;
    }

    /** Key released (or focus lost): ease back, or stop at once. */
    public void release(boolean smoothReturn) {
        if (!held) return;
        held = false;
        returning = smoothReturn;
    }

    /** Mouse movement while held; same scale as {@code Entity.setAngles}. */
    public void turn(float dx, float dy) {
        yaw = (float) (yaw + dx * 0.15);
        pitch = clampPitch((float) (pitch - dy * 0.15));
    }

    /** Advance the return animation towards the player's current rotation. */
    public void update(float dt, float playerYaw, float playerPitch, float speed) {
        if (!returning) return;
        float dYaw = wrapDegrees(playerYaw - yaw);
        float dPitch = clampPitch(playerPitch) - pitch;
        if (Math.abs(dYaw) < DONE && Math.abs(dPitch) < DONE) {
            returning = false;
            return;
        }
        if (!(dt > 0f)) return;
        float k = (float) (1.0 - Math.exp(-speed * Math.min(dt, MAX_DT)));
        yaw += dYaw * k;
        pitch += dPitch * k;
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
