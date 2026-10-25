package com.kestrel.rendering;

/**
 * Kestrel never changes the field of view dynamically.
 *
 * <p>In Minecraft 1.8.9 (Forge 11.15.1.2318) every dynamic FOV change goes
 * through {@code EntityRenderer.getFOVModifier(float partialTicks, boolean useFOVSetting)}:
 * <ol>
 *   <li>{@code fovModifierHand}, interpolated each frame from
 *       {@code AbstractClientPlayer.getFovModifier()}: sprinting, Speed and
 *       Slowness, any other movement-speed modifier, flying (x1.1), drawing a
 *       bow, and anything changed through Forge's {@code FOVUpdateEvent};</li>
 *   <li>the zoom while the player is dying ({@code deathTime});</li>
 *   <li>the 60/70 narrowing while the camera is in water;</li>
 *   <li>Forge's {@code EntityViewRenderEvent.FOVModifier} hook
 *       ({@code ForgeHooksClient.getFOVModifier}), used by other mods.</li>
 * </ol>
 * {@link com.kestrel.mixin.MixinEntityRenderer} replaces that method's result
 * with {@link #fov}: the player's own FOV setting for the world view and the
 * fixed vanilla 70 for the held-item view, so none of the above apply. Only
 * the camera's field of view changes: movement, potions, bows and flying work
 * exactly as before, and the FOV slider in Video Settings keeps working.
 * There is deliberately no setting to turn dynamic FOV back on.
 */
public final class StaticFov {
    /** Field of view vanilla uses for the held-item view (useFOVSetting == false). */
    public static final float HAND_FOV = 70.0f;
    /** Vanilla's developer debug camera ({@code EntityRenderer.debugView}); not a gameplay effect. */
    public static final float DEBUG_VIEW_FOV = 90.0f;

    private StaticFov() {
    }

    /**
     * Field of view for one frame.
     *
     * @param debugView     vanilla's debug camera flag
     * @param useFovSetting true for the world view, false for the held-item view
     * @param fovSetting    the player's configured FOV ({@code GameSettings.fovSetting}), used as-is
     */
    public static float fov(boolean debugView, boolean useFovSetting, float fovSetting) {
        if (debugView) {
            return DEBUG_VIEW_FOV;
        }
        return useFovSetting ? fovSetting : HAND_FOV;
    }
}
