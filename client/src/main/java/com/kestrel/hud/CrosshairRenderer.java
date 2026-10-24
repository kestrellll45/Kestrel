package com.kestrel.hud;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.ScaledResolution;
import net.minecraft.client.renderer.GlStateManager;
import net.minecraftforge.client.event.RenderGameOverlayEvent;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

/**
 * Custom crosshair. When disabled the vanilla crosshair is untouched and this
 * class does no work beyond one boolean check.
 */
public final class CrosshairRenderer {
    private final KestrelClient client;

    public CrosshairRenderer(KestrelClient client) {
        this.client = client;
    }

    @SubscribeEvent
    public void onOverlay(RenderGameOverlayEvent.Pre event) {
        if (event.type != RenderGameOverlayEvent.ElementType.CROSSHAIRS) return;
        ClientConfig.Crosshair c = client.config().crosshair;
        if (!c.custom) return;
        Minecraft mc = Minecraft.getMinecraft();
        event.setCanceled(true);
        if (c.hideInThirdPerson && mc.gameSettings.thirdPersonView != 0) return;
        draw(c, event.resolution);
    }

    /** Draw centred on the screen; sizes are in real pixels, not GUI units. */
    public static void draw(ClientConfig.Crosshair c, ScaledResolution res) {
        float sf = res.getScaleFactor();
        GlStateManager.pushMatrix();
        GlStateManager.translate(res.getScaledWidth() / 2f, res.getScaledHeight() / 2f, 0);
        GlStateManager.scale(1f / sf, 1f / sf, 1f);
        drawAtOrigin(c);
        GlStateManager.popMatrix();
    }

    /** Draw centred at the current origin in pixel units (used by the preview too). */
    public static void drawAtOrigin(ClientConfig.Crosshair c) {
        float t = c.thickness;
        float h = t / 2f;
        float gap = c.gap;
        float len = c.length;
        if (c.outline) {
            shape(c.style, h + 1, gap - 1, len + 2, c.outlineColor);
        }
        shape(c.style, h, gap, len, c.color);
    }

    private static void shape(int style, float h, float gap, float len, int color) {
        boolean cross = style == 0 || style == 2 || style == 3;
        boolean dot = style == 1 || style == 2;
        if (cross) {
            // left, right, bottom; top omitted for the T style.
            RenderUtil.rectF(-gap - len, -h, -gap, h, color);
            RenderUtil.rectF(gap, -h, gap + len, h, color);
            RenderUtil.rectF(-h, gap, h, gap + len, color);
            if (style != 3) {
                RenderUtil.rectF(-h, -gap - len, h, -gap, color);
            }
        }
        if (dot) {
            RenderUtil.rectF(-h, -h, h, h, color);
        }
    }
}
