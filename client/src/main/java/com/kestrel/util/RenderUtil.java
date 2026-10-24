package com.kestrel.util;

import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.renderer.GlStateManager;
import net.minecraft.client.renderer.Tessellator;
import net.minecraft.client.renderer.WorldRenderer;
import net.minecraft.client.renderer.vertex.DefaultVertexFormats;
import net.minecraft.entity.Entity;
import org.lwjgl.opengl.GL11;

/** Small rendering helpers shared by HUD, GUI and effects. */
public final class RenderUtil {
    private RenderUtil() {
    }

    public static void rect(int x1, int y1, int x2, int y2, int argb) {
        Gui.drawRect(x1, y1, x2, y2, argb);
    }

    /** Filled rectangle with 1px chamfered corners (cheap "rounded" look). */
    public static void softRect(int x1, int y1, int x2, int y2, int argb) {
        Gui.drawRect(x1 + 1, y1, x2 - 1, y2, argb);
        Gui.drawRect(x1, y1 + 1, x1 + 1, y2 - 1, argb);
        Gui.drawRect(x2 - 1, y1 + 1, x2, y2 - 1, argb);
    }

    public static void outline(int x1, int y1, int x2, int y2, int argb) {
        Gui.drawRect(x1, y1, x2, y1 + 1, argb);
        Gui.drawRect(x1, y2 - 1, x2, y2, argb);
        Gui.drawRect(x1, y1 + 1, x1 + 1, y2 - 1, argb);
        Gui.drawRect(x2 - 1, y1 + 1, x2, y2 - 1, argb);
    }

    /** Float-precision rectangle (for pixel-exact crosshairs under scaling). */
    public static void rectF(float x1, float y1, float x2, float y2, int argb) {
        float a = (argb >>> 24 & 255) / 255f;
        float r = (argb >> 16 & 255) / 255f;
        float g = (argb >> 8 & 255) / 255f;
        float b = (argb & 255) / 255f;
        Tessellator t = Tessellator.getInstance();
        WorldRenderer w = t.getWorldRenderer();
        GlStateManager.enableBlend();
        GlStateManager.disableTexture2D();
        GlStateManager.tryBlendFuncSeparate(GL11.GL_SRC_ALPHA, GL11.GL_ONE_MINUS_SRC_ALPHA, GL11.GL_ONE, GL11.GL_ZERO);
        w.begin(GL11.GL_QUADS, DefaultVertexFormats.POSITION_COLOR);
        w.pos(x1, y2, 0).color(r, g, b, a).endVertex();
        w.pos(x2, y2, 0).color(r, g, b, a).endVertex();
        w.pos(x2, y1, 0).color(r, g, b, a).endVertex();
        w.pos(x1, y1, 0).color(r, g, b, a).endVertex();
        t.draw();
        GlStateManager.enableTexture2D();
        GlStateManager.disableBlend();
    }

    /** Interpolated render position of an entity. */
    public static double lerp(double prev, double cur, float pt) {
        return prev + (cur - prev) * pt;
    }

    public static double camX(float pt) {
        Entity e = Minecraft.getMinecraft().getRenderViewEntity();
        return e == null ? 0 : lerp(e.lastTickPosX, e.posX, pt);
    }

    public static double camY(float pt) {
        Entity e = Minecraft.getMinecraft().getRenderViewEntity();
        return e == null ? 0 : lerp(e.lastTickPosY, e.posY, pt);
    }

    public static double camZ(float pt) {
        Entity e = Minecraft.getMinecraft().getRenderViewEntity();
        return e == null ? 0 : lerp(e.lastTickPosZ, e.posZ, pt);
    }

    public static int withAlpha(int rgb, float alpha) {
        int a = Math.max(0, Math.min(255, (int) (alpha * 255)));
        return (a << 24) | (rgb & 0xFFFFFF);
    }
}
