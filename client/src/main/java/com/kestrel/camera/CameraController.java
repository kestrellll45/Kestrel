package com.kestrel.camera;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import com.kestrel.input.InputManager;
import net.minecraft.client.Minecraft;
import net.minecraft.client.entity.EntityPlayerSP;
import net.minecraft.client.multiplayer.WorldClient;
import net.minecraft.client.settings.KeyBinding;
import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.fml.client.registry.ClientRegistry;
import net.minecraftforge.fml.common.eventhandler.EventPriority;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;
import org.lwjgl.input.Keyboard;
import org.lwjgl.opengl.Display;

/**
 * Hold-to-zoom (C) and 360-degree free-look (Left Alt).
 *
 * <p>Both are purely visual and client-side:
 * <ul>
 *   <li>Zoom multiplies the world field of view for the current frame inside
 *       the existing static-FOV hook ({@code MixinEntityRenderer}); the FOV
 *       setting itself is never written.</li>
 *   <li>Free-look gives the camera its own yaw/pitch. Mouse movement turns
 *       the camera instead of the player while the key is held
 *       ({@code MixinEntity} on {@code setAngles}); the camera, particle,
 *       name-tag and chunk-culling code read the camera angles through
 *       {@link CameraHooks}. The player's rotation is never changed, so
 *       movement keeps following the body and no rotation packets differ from
 *       vanilla.</li>
 * </ul>
 * The keys are read from their physical state each frame, only while playing
 * with the window focused and no screen open. Losing focus, opening any
 * screen, dying, changing world or disconnecting therefore always releases
 * both; world/player changes also reset them instantly.
 *
 * <p>Keybinds are normal Minecraft key bindings (Controls &gt; Kestrel), saved
 * in options.txt. Because the physical key state is used, they still work
 * when the same key is also bound to something else (vanilla 1.8.9 only
 * delivers a key to one binding).
 */
public final class CameraController implements CameraHooks.TurnHandler {
    private final KestrelClient client;
    private final KeyBinding zoomKey = new KeyBinding("Zoom (hold)", Keyboard.KEY_C, InputManager.CATEGORY);
    private final KeyBinding freeLookKey = new KeyBinding("Free look (hold)", Keyboard.KEY_LMENU, InputManager.CATEGORY);
    private final ZoomState zoom = new ZoomState();
    private final FreeLookState freeLook = new FreeLookState();

    private long lastFrameNanos;
    private WorldClient lastWorld;
    private EntityPlayerSP lastPlayer;
    /** True while we switched to third person for free-look and must switch back. */
    private boolean switchedPerspective;

    public CameraController(KestrelClient client) {
        this.client = client;
    }

    public void registerKeybinds() {
        ClientRegistry.registerKeyBinding(zoomKey);
        ClientRegistry.registerKeyBinding(freeLookKey);
        CameraHooks.setTurnHandler(this);
    }

    @SubscribeEvent
    public void onRenderTick(TickEvent.RenderTickEvent event) {
        if (event.phase != TickEvent.Phase.START) return;
        long now = System.nanoTime();
        float dt = lastFrameNanos == 0L ? 0f : (now - lastFrameNanos) / 1.0e9f;
        lastFrameNanos = now;

        Minecraft mc = Minecraft.getMinecraft();
        EntityPlayerSP player = mc.thePlayer;
        if (mc.theWorld != lastWorld || player != lastPlayer) {
            // New world, respawn, dimension change or disconnect.
            resetNow(mc);
            lastWorld = mc.theWorld;
            lastPlayer = player;
        }
        if (player == null || mc.theWorld == null) {
            CameraHooks.publish(1f, null, 0f, 0f);
            return;
        }
        ClientConfig cfg = client.config();
        boolean playing = mc.currentScreen == null && mc.inGameHasFocus && Display.isActive()
                && !player.isDead && player.getHealth() > 0f;

        boolean zoomHeld = playing && InputManager.isPhysicallyDown(zoomKey.getKeyCode());
        zoom.update(zoomHeld, cfg.zoom.factor, cfg.zoom.speed, cfg.zoom.smooth, dt);

        boolean lookHeld = playing && !player.isPlayerSleeping() && mc.getRenderViewEntity() == player
                && !mc.gameSettings.debugCamEnable && InputManager.isPhysicallyDown(freeLookKey.getKeyCode());
        if (lookHeld && !freeLook.isHeld()) {
            freeLook.press(player.rotationYaw, player.rotationPitch);
            if (cfg.freeLook.thirdPerson && mc.gameSettings.thirdPersonView == 0) {
                setPerspective(mc, 1);
                switchedPerspective = true;
            }
        } else if (!lookHeld && freeLook.isHeld()) {
            // Return on this very frame. If we switched to third person for
            // free-look, going back to first person is already a cut, so the
            // camera goes straight to the player's view with it; otherwise it
            // eases back over at most FreeLookState.RETURN_DURATION.
            boolean cut = switchedPerspective;
            freeLook.release(cfg.freeLook.smoothReturn && !cut);
            if (cut) {
                restorePerspective(mc);
            }
        }
        freeLook.update(dt, player.rotationYaw, player.rotationPitch, FreeLookState.RETURN_DURATION);
        if (switchedPerspective && !freeLook.overrides()) {
            restorePerspective(mc);
        }
        CameraHooks.publish(zoom.fovMultiplier(), freeLook.overrides() ? player : null,
                freeLook.yaw(), freeLook.pitch());
    }

    /** Hide the crosshair while free-looking: it would not show where you aim. */
    @SubscribeEvent(priority = EventPriority.HIGH)
    public void onOverlay(RenderGameOverlayEvent.Pre event) {
        if (event.type == RenderGameOverlayEvent.ElementType.CROSSHAIRS && freeLook.overrides()) {
            event.setCanceled(true);
        }
    }

    /** Mouse look (from {@code Entity.setAngles}); render thread only. */
    @Override
    public boolean turn(Object entity, float yaw, float pitch) {
        Minecraft mc = Minecraft.getMinecraft();
        if (entity == null || entity != mc.thePlayer) return false;
        float scale = client.config().zoom.scaleSensitivity ? zoom.fovMultiplier() : 1f;
        if (freeLook.isHeld()) {
            freeLook.turn(yaw * scale, pitch * scale);
            return true;
        }
        if (scale != 1f) {
            mc.thePlayer.setAngles(yaw * scale, pitch * scale); // re-entrant call runs vanilla
            return true;
        }
        return false;
    }

    private void resetNow(Minecraft mc) {
        zoom.reset();
        freeLook.reset();
        if (switchedPerspective) {
            restorePerspective(mc);
        }
        CameraHooks.publish(1f, null, 0f, 0f);
    }

    private void restorePerspective(Minecraft mc) {
        switchedPerspective = false;
        // Leave it alone if the player changed perspective (F5) meanwhile.
        if (mc.gameSettings.thirdPersonView == 1) {
            setPerspective(mc, 0);
        }
    }

    private static void setPerspective(Minecraft mc, int view) {
        mc.gameSettings.thirdPersonView = view;
        if (mc.renderGlobal != null) {
            mc.renderGlobal.setDisplayListEntitiesDirty();
        }
    }

    public ZoomState zoom() {
        return zoom;
    }

    public FreeLookState freeLook() {
        return freeLook;
    }
}
