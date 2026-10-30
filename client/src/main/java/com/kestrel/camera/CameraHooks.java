package com.kestrel.camera;

/**
 * Per-frame camera values read by the rendering mixins. Written once per
 * frame by {@link CameraController} on the render thread before anything is
 * drawn, and only read on that thread. No Minecraft types here, so this is
 * unit-tested and safe to load from any mixin.
 */
public final class CameraHooks {
    private static float fovMultiplier = 1.0f;
    /** The entity whose camera angles are overridden (the local player), or null. */
    private static Object overrideEntity;
    private static float yaw;
    private static float pitch;
    /** Mouse turn handler; null = vanilla behaviour. */
    private static TurnHandler turnHandler;
    private static boolean inTurn;

    private CameraHooks() {
    }

    /** Handles mouse-look for the local player. */
    public interface TurnHandler {
        /**
         * @return true if the turn was fully handled (vanilla must not run)
         */
        boolean turn(Object entity, float yaw, float pitch);
    }

    public static float fovMultiplier() {
        return fovMultiplier;
    }

    static void publish(float zoomMultiplier, Object entity, float camYaw, float camPitch) {
        fovMultiplier = zoomMultiplier > 0f && zoomMultiplier <= 1f ? zoomMultiplier : 1f;
        overrideEntity = entity;
        yaw = camYaw;
        pitch = camPitch;
    }

    static void setTurnHandler(TurnHandler h) {
        turnHandler = h;
    }

    public static boolean overrides(Object entity) {
        return entity != null && entity == overrideEntity;
    }

    /** Camera yaw for {@code entity}: the free-look yaw, or {@code original}. */
    public static float yaw(Object entity, float original) {
        return entity != null && entity == overrideEntity ? yaw : original;
    }

    public static float pitch(Object entity, float original) {
        return entity != null && entity == overrideEntity ? pitch : original;
    }

    /**
     * Called at the start of {@code Entity.setAngles}. Re-entrant calls (the
     * handler calling setAngles with scaled values) always run vanilla.
     *
     * @return true to cancel the vanilla method
     */
    public static boolean onSetAngles(Object entity, float yaw, float pitch) {
        TurnHandler h = turnHandler;
        if (h == null || inTurn) return false;
        inTurn = true;
        try {
            return h.turn(entity, yaw, pitch);
        } finally {
            inTurn = false;
        }
    }
}
