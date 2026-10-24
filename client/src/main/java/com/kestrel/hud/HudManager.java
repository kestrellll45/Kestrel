package com.kestrel.hud;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import com.kestrel.hud.elements.ArmorElement;
import com.kestrel.hud.elements.CoordsElement;
import com.kestrel.hud.elements.CpsElement;
import com.kestrel.hud.elements.FpsElement;
import com.kestrel.hud.elements.FrameStatsElement;
import com.kestrel.hud.elements.KeystrokesElement;
import com.kestrel.hud.elements.PotionElement;
import com.kestrel.hud.elements.StatusElement;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.ScaledResolution;
import net.minecraft.client.renderer.GlStateManager;
import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** Holds HUD elements and draws them after the vanilla overlay. */
public final class HudManager {
    private final KestrelClient client;
    private final List<HudElement> elements = new ArrayList<HudElement>();
    private boolean forceRefresh = true;
    private final float[] posScratch = new float[2];

    public HudManager(KestrelClient client) {
        this.client = client;
        add(new FpsElement(client));
        add(new CpsElement(client));
        add(new KeystrokesElement(client));
        add(new ArmorElement(client));
        add(new PotionElement(client));
        add(new CoordsElement(client));
        add(new StatusElement(client));
        add(new FrameStatsElement(client));
    }

    private void add(HudElement e) {
        elements.add(e);
        e.cfg(); // register defaults in the config
    }

    public List<HudElement> elements() {
        return Collections.unmodifiableList(elements);
    }

    public void invalidate() {
        forceRefresh = true;
    }

    @SubscribeEvent
    public void onOverlay(RenderGameOverlayEvent.Post event) {
        if (event.type != RenderGameOverlayEvent.ElementType.ALL) return;
        Minecraft mc = Minecraft.getMinecraft();
        if (mc.gameSettings.hideGUI || mc.gameSettings.showDebugInfo || mc.currentScreen instanceof com.kestrel.gui.HudEditorScreen) {
            return;
        }
        renderAll(mc, event.resolution, false);
    }

    /** Draw every enabled element (or all, dimmed, in preview mode). */
    public void renderAll(Minecraft mc, ScaledResolution res, boolean preview) {
        long now = System.currentTimeMillis();
        int interval = client.config().performance.hudRefreshMs;
        boolean force = forceRefresh;
        forceRefresh = false;
        for (int i = 0; i < elements.size(); i++) {
            HudElement e = elements.get(i);
            ClientConfig.HudElementConfig c = e.cfg();
            if (!c.enabled && !preview) continue;
            e.maybeRefresh(mc, now, e.realtime() ? 0 : interval, force);
            int w = e.width(preview);
            int h = e.height(preview);
            if (w <= 0 || h <= 0) continue;
            float[] pos = screenPos(e, res, w, h, posScratch);
            GlStateManager.pushMatrix();
            GlStateManager.translate(pos[0], pos[1], 0);
            GlStateManager.scale(c.scale, c.scale, 1);
            if (preview && !c.enabled) {
                GlStateManager.color(1, 1, 1, 0.4f);
            }
            e.render(mc, preview);
            GlStateManager.color(1, 1, 1, 1);
            GlStateManager.popMatrix();
        }
    }

    /** Top-left screen position, clamped so the element stays visible. */
    public static float[] screenPos(HudElement e, ScaledResolution res, int w, int h, float[] out) {
        ClientConfig.HudElementConfig c = e.cfg();
        float sw = res.getScaledWidth();
        float sh = res.getScaledHeight();
        float x = c.x * sw;
        float y = c.y * sh;
        float maxX = Math.max(0, sw - w * c.scale);
        float maxY = Math.max(0, sh - h * c.scale);
        out[0] = Math.max(0, Math.min(maxX, x));
        out[1] = Math.max(0, Math.min(maxY, y));
        return out;
    }
}
