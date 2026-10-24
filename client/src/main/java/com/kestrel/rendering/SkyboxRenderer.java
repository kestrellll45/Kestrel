package com.kestrel.rendering;

import com.kestrel.config.ClientConfig;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.WorldClient;
import net.minecraft.client.renderer.GlStateManager;
import net.minecraft.client.renderer.RenderHelper;
import net.minecraft.client.renderer.Tessellator;
import net.minecraft.client.renderer.WorldRenderer;
import net.minecraft.client.renderer.vertex.DefaultVertexFormats;
import net.minecraft.util.MathHelper;
import net.minecraft.util.ResourceLocation;
import net.minecraftforge.client.IRenderHandler;
import org.lwjgl.opengl.GL11;

/**
 * Textured skybox installed through Forge's {@code WorldProvider#setSkyRenderer},
 * which makes vanilla {@code RenderGlobal.renderSky} delegate to us. One draw
 * call per face, no shaders; state handling mirrors vanilla's End sky.
 *
 * <p>Texture layout: a 3x2 atlas of square faces —
 * row 0: north, east, south; row 1: west, up, down. Side faces are oriented
 * "as seen from inside, standing upright", so a horizon drawn across the
 * middle of each side face lines up all around. The exact corner table below
 * is the format contract: tools/gen_assets.py samples each texel's 3D
 * direction from the same table, which makes the bundled skies seamless.
 * Documented for custom skies in documentation/SKIES.md.
 */
public final class SkyboxRenderer extends IRenderHandler {
    private static final double R = 100.0;

    private final SkyManager manager;

    SkyboxRenderer(SkyManager manager) {
        this.manager = manager;
    }

    @Override
    public void render(float partialTicks, WorldClient world, Minecraft mc) {
        ResourceLocation tex = manager.texture();
        ClientConfig.Sky cfg = manager.config();
        if (tex == null) return; // unloaded mid-frame; vanilla clear colour remains

        float b = cfg.brightness;
        if (cfg.followDayCycle) {
            float angle = world.getCelestialAngle(partialTicks);
            float day = MathHelper.cos(angle * (float) Math.PI * 2.0F) * 2.0F + 0.5F;
            b *= Math.max(0.15F, Math.min(1.0F, day));
        }
        int c = Math.max(0, Math.min(255, (int) (b * 255)));

        GlStateManager.disableFog();
        GlStateManager.disableAlpha();
        GlStateManager.enableBlend();
        GlStateManager.tryBlendFuncSeparate(GL11.GL_SRC_ALPHA, GL11.GL_ONE_MINUS_SRC_ALPHA, GL11.GL_ONE, GL11.GL_ZERO);
        RenderHelper.disableStandardItemLighting();
        GlStateManager.depthMask(false);
        GlStateManager.disableCull(); // we are inside the box; winding must not matter
        GlStateManager.enableTexture2D();
        mc.getTextureManager().bindTexture(tex);

        GlStateManager.pushMatrix();
        if (cfg.rotate && cfg.rotationSpeed > 0) {
            // World time keeps rotation smooth and stable across frame-rate changes.
            float seconds = (world.getTotalWorldTime() + partialTicks) / 20.0F;
            GlStateManager.rotate((seconds * cfg.rotationSpeed) % 360.0F, 0.0F, 1.0F, 0.0F);
        }
        Tessellator t = Tessellator.getInstance();
        WorldRenderer w = t.getWorldRenderer();
        w.begin(GL11.GL_QUADS, DefaultVertexFormats.POSITION_TEX_COLOR);
        // Each face: four corners as (x, y, z) with (u, v) in face space,
        // listed top-left, bottom-left, bottom-right, top-right as seen from inside.
        // north (-Z): right is +X
        face(w, 0, 0, c, -R, R, -R, -R, -R, -R, R, -R, -R, R, R, -R);
        // east (+X): right is +Z
        face(w, 1, 0, c, R, R, -R, R, -R, -R, R, -R, R, R, R, R);
        // south (+Z): right is -X
        face(w, 2, 0, c, R, R, R, R, -R, R, -R, -R, R, -R, R, R);
        // west (-X): right is -Z
        face(w, 0, 1, c, -R, R, R, -R, -R, R, -R, -R, -R, -R, R, -R);
        // up (+Y): image top edge at +Z, left edge at -X
        face(w, 1, 1, c, -R, R, R, -R, R, -R, R, R, -R, R, R, R);
        // down (-Y): image top edge at -Z, left edge at -X
        face(w, 2, 1, c, -R, -R, -R, -R, -R, R, R, -R, R, R, -R, -R);
        t.draw();
        GlStateManager.popMatrix();

        GlStateManager.enableCull();
        GlStateManager.depthMask(true);
        GlStateManager.enableAlpha();
        GlStateManager.disableBlend();
        GlStateManager.enableFog();
    }

    private static void face(WorldRenderer w, int col, int row, int c,
                             double x0, double y0, double z0,  // top-left
                             double x1, double y1, double z1,  // bottom-left
                             double x2, double y2, double z2,  // bottom-right
                             double x3, double y3, double z3) { // top-right
        // Inset half a texel-ish to avoid bleeding between atlas cells.
        double e = 0.0015;
        double u0 = col / 3.0 + e;
        double u1 = (col + 1) / 3.0 - e;
        double v0 = row / 2.0 + e;
        double v1 = (row + 1) / 2.0 - e;
        w.pos(x0, y0, z0).tex(u0, v0).color(c, c, c, 255).endVertex();
        w.pos(x1, y1, z1).tex(u0, v1).color(c, c, c, 255).endVertex();
        w.pos(x2, y2, z2).tex(u1, v1).color(c, c, c, 255).endVertex();
        w.pos(x3, y3, z3).tex(u1, v0).color(c, c, c, 255).endVertex();
    }
}
