package com.kestrel.effects;

import net.minecraft.block.Block;
import net.minecraft.client.multiplayer.WorldClient;
import net.minecraft.client.renderer.GlStateManager;
import net.minecraft.client.renderer.Tessellator;
import net.minecraft.client.renderer.WorldRenderer;
import net.minecraft.client.renderer.vertex.DefaultVertexFormats;
import net.minecraft.init.Blocks;
import net.minecraft.util.EnumParticleTypes;
import org.lwjgl.opengl.GL11;

import java.util.Random;

/**
 * The second set of kill effects. Same rules as {@link Effects}: vanilla
 * particles go through the player's particle setting and the per-effect
 * budget; client-drawn geometry is a few hundred vertices at most, is drawn
 * only while the effect lives, and allocates nothing per frame.
 */
final class MoreEffects {
    private static final Random RNG = new Random();

    private MoreEffects() {
    }

    // ---- shared helpers --------------------------------------------------------

    static float[] hue(float h) {
        java.awt.Color c = java.awt.Color.getHSBColor(h, 0.85f, 1.0f);
        return new float[] {Math.max(0.001f, c.getRed() / 255f), c.getGreen() / 255f, c.getBlue() / 255f};
    }

    static float r(int argb) {
        return Effects.red(argb);
    }

    static float g(int argb) {
        return Effects.green(argb);
    }

    static float b(int argb) {
        return Effects.blue(argb);
    }

    /** Additive, untextured, no depth writes: glowing geometry. */
    static void beginGlow(double ox, double oy, double oz) {
        GlStateManager.pushMatrix();
        GlStateManager.translate(ox, oy, oz);
        GlStateManager.disableTexture2D();
        GlStateManager.disableLighting();
        GlStateManager.disableCull();
        GlStateManager.enableBlend();
        GlStateManager.blendFunc(GL11.GL_SRC_ALPHA, GL11.GL_ONE);
        GlStateManager.depthMask(false);
    }

    /** Normal alpha blending (solid-looking pieces such as confetti or voxels). */
    static void beginSolid(double ox, double oy, double oz) {
        GlStateManager.pushMatrix();
        GlStateManager.translate(ox, oy, oz);
        GlStateManager.disableTexture2D();
        GlStateManager.disableLighting();
        GlStateManager.disableCull();
        GlStateManager.enableBlend();
        GlStateManager.blendFunc(GL11.GL_SRC_ALPHA, GL11.GL_ONE_MINUS_SRC_ALPHA);
    }

    static void end() {
        GL11.glLineWidth(1.0f);
        GlStateManager.depthMask(true);
        GlStateManager.blendFunc(GL11.GL_SRC_ALPHA, GL11.GL_ONE_MINUS_SRC_ALPHA);
        GlStateManager.disableBlend();
        GlStateManager.enableCull();
        GlStateManager.enableTexture2D();
        GlStateManager.popMatrix();
    }

    /** Horizontal ring (annulus) at height y, as a quad strip of {@code seg} segments. */
    static void ring(WorldRenderer wr, double y, double r0, double r1, int seg, float red, float green, float blue, float a0, float a1) {
        wr.begin(GL11.GL_QUAD_STRIP, DefaultVertexFormats.POSITION_COLOR);
        for (int i = 0; i <= seg; i++) {
            double t = 2 * Math.PI * i / seg;
            double cs = Math.cos(t);
            double sn = Math.sin(t);
            wr.pos(cs * r0, y, sn * r0).color(red, green, blue, a0).endVertex();
            wr.pos(cs * r1, y, sn * r1).color(red, green, blue, a1).endVertex();
        }
        Tessellator.getInstance().draw();
    }

    /** Small axis-aligned cube centred at (x,y,z). Call between wr.begin(GL_QUADS) and draw. */
    static void cube(WorldRenderer wr, double x, double y, double z, double s, float red, float green, float blue, float a) {
        double x0 = x - s, x1 = x + s, y0 = y - s, y1 = y + s, z0 = z - s, z1 = z + s;
        float d = 0.75f; // darker sides
        quad(wr, x0, y1, z0, x1, y1, z0, x1, y1, z1, x0, y1, z1, red, green, blue, a);
        quad(wr, x0, y0, z0, x0, y0, z1, x1, y0, z1, x1, y0, z0, red * 0.6f, green * 0.6f, blue * 0.6f, a);
        quad(wr, x0, y0, z0, x1, y0, z0, x1, y1, z0, x0, y1, z0, red * d, green * d, blue * d, a);
        quad(wr, x0, y0, z1, x0, y1, z1, x1, y1, z1, x1, y0, z1, red * d, green * d, blue * d, a);
        quad(wr, x0, y0, z0, x0, y1, z0, x0, y1, z1, x0, y0, z1, red * 0.85f, green * 0.85f, blue * 0.85f, a);
        quad(wr, x1, y0, z0, x1, y0, z1, x1, y1, z1, x1, y1, z0, red * 0.85f, green * 0.85f, blue * 0.85f, a);
    }

    static void quad(WorldRenderer wr, double ax, double ay, double az, double bx, double by, double bz,
                     double cx, double cy, double cz, double dx, double dy, double dz, float red, float green, float blue, float a) {
        wr.pos(ax, ay, az).color(red, green, blue, a).endVertex();
        wr.pos(bx, by, bz).color(red, green, blue, a).endVertex();
        wr.pos(cx, cy, cz).color(red, green, blue, a).endVertex();
        wr.pos(dx, dy, dz).color(red, green, blue, a).endVertex();
    }

    /** Simple ballistic pieces (confetti, voxels) with interpolated rendering. */
    static final class Pieces {
        final int n;
        final double[] px, py, pz, vx, vy, vz, ox, oy, oz;
        final float[] cr, cg, cb, spin;

        Pieces(int n) {
            this.n = n;
            px = new double[n];
            py = new double[n];
            pz = new double[n];
            vx = new double[n];
            vy = new double[n];
            vz = new double[n];
            ox = new double[n];
            oy = new double[n];
            oz = new double[n];
            cr = new float[n];
            cg = new float[n];
            cb = new float[n];
            spin = new float[n];
        }

        void step(double gravity, double drag) {
            for (int i = 0; i < n; i++) {
                ox[i] = px[i];
                oy[i] = py[i];
                oz[i] = pz[i];
                px[i] += vx[i];
                py[i] += vy[i];
                pz[i] += vz[i];
                vx[i] *= drag;
                vz[i] *= drag;
                vy[i] = vy[i] * drag - gravity;
            }
        }

        double ix(int i, float pt) {
            return ox[i] + (px[i] - ox[i]) * pt;
        }

        double iy(int i, float pt) {
            return oy[i] + (py[i] - oy[i]) * pt;
        }

        double iz(int i, float pt) {
            return oz[i] + (pz[i] - oz[i]) * pt;
        }
    }

    // ---- effects ---------------------------------------------------------------

    /** Three rockets rise and burst into coloured shells. */
    static final class Fireworks extends ActiveEffect {
        private final int color;
        private final double[] ox = new double[3];
        private final double[] oz = new double[3];
        private final int[] burstAt = {7, 10, 13};

        Fireworks(double x, double y, double z, float i, int b, int color) {
            super(x, y, z, i, b);
            this.color = color;
            for (int k = 0; k < 3; k++) {
                ox[k] = RNG.nextGaussian() * 0.8;
                oz[k] = RNG.nextGaussian() * 0.8;
            }
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            for (int k = 0; k < 3; k++) {
                double h = y + 1 + Math.min(step, burstAt[k]) * 0.45;
                if (step < burstAt[k]) {
                    if (allowance(1) == 1) {
                        w.spawnParticle(EnumParticleTypes.FIREWORKS_SPARK, x + ox[k], h, z + oz[k], 0, -0.05, 0);
                    }
                } else if (step == burstAt[k]) {
                    float[] col = k == 0 ? new float[] {r(color), g(color), b(color)} : hue(k * 0.33f + 0.1f);
                    int n = allowance((int) (22 * intensity));
                    for (int p = 0; p < n; p++) {
                        double u = RNG.nextDouble() * 2 - 1;
                        double t = RNG.nextDouble() * Math.PI * 2;
                        double s = Math.sqrt(1 - u * u);
                        double sp = 0.28;
                        if (p % 2 == 0) {
                            w.spawnParticle(EnumParticleTypes.FIREWORKS_SPARK, x + ox[k], h, z + oz[k], s * Math.cos(t) * sp, u * sp, s * Math.sin(t) * sp);
                        } else {
                            w.spawnParticle(EnumParticleTypes.REDSTONE, x + ox[k] + s * Math.cos(t) * 1.4, h + u * 1.4, z + oz[k] + s * Math.sin(t) * 1.4, col[0], col[1], col[2]);
                        }
                    }
                }
            }
        }

        @Override
        int lifetimeTicks() {
            return 16;
        }
    }

    /** A rolling cloud of smoke spreading along the ground. */
    static final class Smoke extends ActiveEffect {
        Smoke(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            int n = allowance((int) (8 * intensity));
            double r = 0.3 + step * 0.22;
            for (int k = 0; k < n; k++) {
                double a = RNG.nextDouble() * Math.PI * 2;
                w.spawnParticle(k % 2 == 0 ? EnumParticleTypes.SMOKE_LARGE : EnumParticleTypes.CLOUD,
                        x + Math.cos(a) * r, y + 0.2 + RNG.nextDouble() * (1.6 - step * 0.1), z + Math.sin(a) * r,
                        Math.cos(a) * 0.05, 0.02, Math.sin(a) * 0.05);
            }
        }

        @Override
        int lifetimeTicks() {
            return 10;
        }
    }

    /** Pale blue soul wisps spiralling slowly upward. */
    static final class Soul extends ActiveEffect {
        Soul(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            int n = allowance(Math.max(2, (int) (4 * intensity)));
            for (int k = 0; k < n; k++) {
                double a = step * 0.45 + k * (2 * Math.PI / n);
                double rr = 0.45 + 0.15 * Math.sin(step * 0.3);
                // SPELL_MOB takes its colour from the velocity arguments.
                w.spawnParticle(EnumParticleTypes.SPELL_MOB, x + Math.cos(a) * rr, y + 0.3 + step * 0.1, z + Math.sin(a) * rr,
                        0.45, 0.85, 1.0);
            }
            if (step % 4 == 0 && allowance(1) == 1) {
                w.spawnParticle(EnumParticleTypes.ENCHANTMENT_TABLE, x, y + 2.4, z, 0, -1.0, 0);
            }
        }

        @Override
        void render(float pt, double cx, double cy, double cz) {
            // A faint ghostly column fading upward.
            float life = (age + pt) / lifetimeTicks();
            float a = 0.25f * (1 - life);
            if (a <= 0.01f) return;
            beginGlow(x - cx, y - cy, z - cz);
            Tessellator t = Tessellator.getInstance();
            WorldRenderer wr = t.getWorldRenderer();
            double h = 0.2 + life * 2.6;
            for (int s = 0; s < 6; s++) {
                ring(wr, h * s / 6.0, 0.0, 0.5 - s * 0.06, 20, 0.45f, 0.85f, 1.0f, a * (1 - s / 6f), 0f);
            }
            end();
        }

        @Override
        int lifetimeTicks() {
            return 24;
        }
    }

    /** Ender particles collapsing into the victim with a purple flash. */
    static final class Ender extends ActiveEffect {
        Ender(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            int n = allowance((int) (14 * intensity));
            for (int k = 0; k < n; k++) {
                // Portal particles drift back toward their spawn offset origin.
                w.spawnParticle(EnumParticleTypes.PORTAL, x, y + 1, z, RNG.nextGaussian() * 1.6, RNG.nextGaussian() * 1.2, RNG.nextGaussian() * 1.6);
            }
            if (step == 6) {
                int f = allowance((int) (16 * intensity));
                for (int k = 0; k < f; k++) {
                    w.spawnParticle(EnumParticleTypes.REDSTONE, x + RNG.nextGaussian() * 0.4, y + 1 + RNG.nextGaussian() * 0.6,
                            z + RNG.nextGaussian() * 0.4, 0.7, 0.2, 1.0);
                }
            }
        }

        @Override
        int lifetimeTicks() {
            return 8;
        }
    }

    /** Flickering electric arcs crawling over the body. */
    static final class Shock extends ActiveEffect {
        private static final int ARCS = 6;
        private static final int SEG = 7;
        private final double[] ax = new double[ARCS * (SEG + 1)];
        private final double[] ay = new double[ARCS * (SEG + 1)];
        private final double[] az = new double[ARCS * (SEG + 1)];

        Shock(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
            reshape();
        }

        private void reshape() {
            for (int a = 0; a < ARCS; a++) {
                double ang = RNG.nextDouble() * Math.PI * 2;
                double y0 = RNG.nextDouble() * 1.8;
                double y1 = RNG.nextDouble() * 1.8;
                for (int s = 0; s <= SEG; s++) {
                    double t = s / (double) SEG;
                    double jitter = (s == 0 || s == SEG) ? 0 : 0.18;
                    double aa = ang + t * 1.6;
                    int i = a * (SEG + 1) + s;
                    ax[i] = Math.cos(aa) * 0.45 + RNG.nextGaussian() * jitter;
                    ay[i] = y0 + (y1 - y0) * t + RNG.nextGaussian() * jitter;
                    az[i] = Math.sin(aa) * 0.45 + RNG.nextGaussian() * jitter;
                }
            }
        }

        @Override
        void tick(WorldClient w) {
            age++;
            reshape();
            int n = allowance(Math.max(1, (int) (3 * intensity)));
            for (int k = 0; k < n; k++) {
                w.spawnParticle(EnumParticleTypes.CRIT_MAGIC, x + RNG.nextGaussian() * 0.4, y + RNG.nextDouble() * 1.8, z + RNG.nextGaussian() * 0.4, 0, 0, 0);
            }
        }

        @Override
        void render(float pt, double cx, double cy, double cz) {
            float life = (age + pt) / lifetimeTicks();
            float alpha = Math.min(1f, intensity) * (1 - life);
            if (alpha <= 0.01f) return;
            beginGlow(x - cx, y - cy, z - cz);
            Tessellator t = Tessellator.getInstance();
            WorldRenderer wr = t.getWorldRenderer();
            for (int pass = 0; pass < 2; pass++) {
                GL11.glLineWidth(pass == 0 ? 5.0f : 1.5f);
                float a = pass == 0 ? alpha * 0.3f : alpha;
                for (int arc = 0; arc < ARCS; arc++) {
                    wr.begin(GL11.GL_LINE_STRIP, DefaultVertexFormats.POSITION_COLOR);
                    for (int s = 0; s <= SEG; s++) {
                        int i = arc * (SEG + 1) + s;
                        wr.pos(ax[i], ay[i], az[i]).color(0.55f, 0.9f, 1.0f, a).endVertex();
                    }
                    t.draw();
                }
            }
            end();
        }

        @Override
        int lifetimeTicks() {
            return 14;
        }
    }

    /** Ice shards burst outward from the body with snow and breaking ice. */
    static final class Ice extends ActiveEffect {
        private static final int SHARDS = 10;
        private final double[] dx = new double[SHARDS];
        private final double[] dy = new double[SHARDS];
        private final double[] dz = new double[SHARDS];

        Ice(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
            for (int s = 0; s < SHARDS; s++) {
                double a = 2 * Math.PI * s / SHARDS + RNG.nextDouble() * 0.4;
                double up = 0.2 + RNG.nextDouble() * 0.9;
                dx[s] = Math.cos(a);
                dy[s] = up;
                dz[s] = Math.sin(a);
            }
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            if (step == 0) {
                int n = allowance((int) (16 * intensity));
                int ice = Block.getStateId(Blocks.packed_ice.getDefaultState());
                for (int k = 0; k < n; k++) {
                    w.spawnParticle(EnumParticleTypes.BLOCK_CRACK, x + RNG.nextGaussian() * 0.3, y + 0.5 + RNG.nextDouble(), z + RNG.nextGaussian() * 0.3,
                            RNG.nextGaussian() * 0.2, 0.2, RNG.nextGaussian() * 0.2, ice);
                }
            }
            int s = allowance(Math.max(1, (int) (4 * intensity)));
            for (int k = 0; k < s; k++) {
                w.spawnParticle(EnumParticleTypes.SNOW_SHOVEL, x + RNG.nextGaussian() * 0.8, y + 0.2 + RNG.nextDouble() * 2, z + RNG.nextGaussian() * 0.8, 0, -0.02, 0);
            }
        }

        @Override
        void render(float pt, double cx, double cy, double cz) {
            float life = (age + pt) / lifetimeTicks();
            float grow = Math.min(1f, (age + pt) / 3f);
            float alpha = 0.75f * (1 - life * life);
            if (alpha <= 0.01f) return;
            beginSolid(x - cx, y + 0.9 - cy, z - cz);
            Tessellator t = Tessellator.getInstance();
            WorldRenderer wr = t.getWorldRenderer();
            wr.begin(GL11.GL_TRIANGLES, DefaultVertexFormats.POSITION_COLOR);
            for (int s = 0; s < SHARDS; s++) {
                double len = (0.9 + (s % 3) * 0.25) * grow;
                double tx = dx[s] * len, ty = dy[s] * len, tz = dz[s] * len;
                double px = -dz[s] * 0.09, pz = dx[s] * 0.09;
                wr.pos(px, 0, pz).color(0.75f, 0.92f, 1.0f, alpha).endVertex();
                wr.pos(-px, 0, -pz).color(0.55f, 0.8f, 1.0f, alpha).endVertex();
                wr.pos(tx, ty, tz).color(0.95f, 1.0f, 1.0f, alpha).endVertex();
                wr.pos(0, 0.1, 0).color(0.7f, 0.9f, 1.0f, alpha).endVertex();
                wr.pos(0, -0.1, 0).color(0.5f, 0.75f, 1.0f, alpha).endVertex();
                wr.pos(tx, ty, tz).color(0.95f, 1.0f, 1.0f, alpha).endVertex();
            }
            t.draw();
            end();
        }

        @Override
        int lifetimeTicks() {
            return 16;
        }
    }

    /** Stacked rainbow rings expanding outward. */
    static final class Rainbow extends ActiveEffect {
        Rainbow(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            int per = allowance(Math.max(4, (int) (14 * intensity)));
            double rad = 0.3 + step * 0.22;
            for (int k = 0; k < per; k++) {
                double a = 2 * Math.PI * k / per;
                float[] col = hue((float) k / per + step * 0.05f);
                w.spawnParticle(EnumParticleTypes.REDSTONE, x + Math.cos(a) * rad, y + 0.3 + (step % 3) * 0.6, z + Math.sin(a) * rad, col[0], col[1], col[2]);
            }
        }

        @Override
        int lifetimeTicks() {
            return 9;
        }
    }

    /** Nether fire: flames, lava drops and dark smoke. */
    static final class Nether extends ActiveEffect {
        Nether(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            if (step == 0) {
                int l = allowance(Math.max(2, (int) (8 * intensity)));
                for (int k = 0; k < l; k++) {
                    w.spawnParticle(EnumParticleTypes.LAVA, x, y + 0.6, z, 0, 0, 0);
                }
            }
            int n = allowance(Math.max(2, (int) (5 * intensity)));
            for (int k = 0; k < n; k++) {
                double a = RNG.nextDouble() * Math.PI * 2;
                double rr = RNG.nextDouble() * 0.6;
                w.spawnParticle(k % 3 == 0 ? EnumParticleTypes.SMOKE_LARGE : EnumParticleTypes.FLAME,
                        x + Math.cos(a) * rr, y + RNG.nextDouble() * 0.4, z + Math.sin(a) * rr, 0, 0.04 + RNG.nextDouble() * 0.06, 0);
            }
            if (step % 3 == 0 && allowance(1) == 1) {
                w.spawnParticle(EnumParticleTypes.DRIP_LAVA, x + RNG.nextGaussian() * 0.5, y + 2.4, z + RNG.nextGaussian() * 0.5, 0, 0, 0);
            }
        }

        @Override
        void render(float pt, double cx, double cy, double cz) {
            float life = (age + pt) / lifetimeTicks();
            float a = 0.45f * (1 - life);
            if (a <= 0.01f) return;
            beginGlow(x - cx, y - cy, z - cz);
            ring(Tessellator.getInstance().getWorldRenderer(), 0.05, 0.0, 0.6 + life * 1.2, 24, 1.0f, 0.35f, 0.05f, a, 0f);
            end();
        }

        @Override
        int lifetimeTicks() {
            return 16;
        }
    }

    /** Dragon breath: two purple wing arcs sweeping upward. */
    static final class Dragon extends ActiveEffect {
        Dragon(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            double sweep = step / (double) lifetimeTicks();
            int per = allowance(Math.max(2, (int) (6 * intensity)));
            for (int k = 0; k < per; k++) {
                double t = k / (double) Math.max(1, per - 1);
                double span = 0.4 + t * 1.8;
                double lift = Math.sin(t * Math.PI) * 0.6 + sweep * 1.2;
                for (int side = -1; side <= 1; side += 2) {
                    w.spawnParticle(k % 2 == 0 ? EnumParticleTypes.SPELL_WITCH : EnumParticleTypes.PORTAL,
                            x + side * span * Math.cos(sweep * 0.8), y + 1 + lift, z + side * span * Math.sin(sweep * 0.8) * 0.3, 0, 0, 0);
                }
            }
        }

        @Override
        int lifetimeTicks() {
            return 14;
        }
    }

    /** Pixelated explosion: coloured voxels flying out and bouncing on the ground. */
    static final class Pixel extends ActiveEffect {
        private final Pieces p;

        Pixel(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
            int n = Math.max(8, Math.min(Math.min(b, 120), (int) (60 * i)));
            p = new Pieces(n);
            for (int k = 0; k < n; k++) {
                double a = RNG.nextDouble() * Math.PI * 2;
                double sp = 0.12 + RNG.nextDouble() * 0.22;
                p.px[k] = p.ox[k] = RNG.nextGaussian() * 0.25;
                p.py[k] = p.oy[k] = 0.8 + RNG.nextDouble() * 0.8;
                p.pz[k] = p.oz[k] = RNG.nextGaussian() * 0.25;
                p.vx[k] = Math.cos(a) * sp;
                p.vy[k] = 0.15 + RNG.nextDouble() * 0.25;
                p.vz[k] = Math.sin(a) * sp;
                float[] col = (k % 4 == 0) ? new float[] {1f, 1f, 1f} : hue((float) RNG.nextDouble());
                p.cr[k] = col[0];
                p.cg[k] = col[1];
                p.cb[k] = col[2];
            }
            particlesSpawned = n;
        }

        @Override
        void tick(WorldClient w) {
            age++;
            p.step(0.04, 0.96);
            for (int k = 0; k < p.n; k++) {
                if (p.py[k] < 0.06) { // bounce on the victim's floor level
                    p.py[k] = 0.06;
                    p.vy[k] = -p.vy[k] * 0.4;
                    p.vx[k] *= 0.7;
                    p.vz[k] *= 0.7;
                }
            }
        }

        @Override
        void render(float pt, double cx, double cy, double cz) {
            float life = (age + pt) / lifetimeTicks();
            float alpha = life < 0.7f ? 1f : (1 - life) / 0.3f;
            if (alpha <= 0.01f) return;
            beginSolid(x - cx, y - cy, z - cz);
            Tessellator t = Tessellator.getInstance();
            WorldRenderer wr = t.getWorldRenderer();
            wr.begin(GL11.GL_QUADS, DefaultVertexFormats.POSITION_COLOR);
            for (int k = 0; k < p.n; k++) {
                cube(wr, p.ix(k, pt), p.iy(k, pt), p.iz(k, pt), 0.06, p.cr[k], p.cg[k], p.cb[k], alpha);
            }
            t.draw();
            end();
        }

        @Override
        int lifetimeTicks() {
            return 30;
        }
    }

    /** Glowing energy shockwave in your colour with a vertical beam. */
    static final class Energy extends ActiveEffect {
        private final int color;

        Energy(double x, double y, double z, float i, int b, int color) {
            super(x, y, z, i, b);
            this.color = color;
        }

        @Override
        void tick(WorldClient w) {
            if (age++ == 0) {
                int n = allowance((int) (16 * intensity));
                for (int k = 0; k < n; k++) {
                    w.spawnParticle(EnumParticleTypes.CRIT_MAGIC, x, y + 1, z, RNG.nextGaussian() * 0.5, RNG.nextGaussian() * 0.3, RNG.nextGaussian() * 0.5);
                }
            }
        }

        @Override
        void render(float pt, double cx, double cy, double cz) {
            float life = (age + pt) / lifetimeTicks();
            float alpha = (1 - life) * Math.min(1f, intensity);
            if (alpha <= 0.01f) return;
            float cr = r(color), cg = g(color), cb = b(color);
            beginGlow(x - cx, y - cy, z - cz);
            Tessellator t = Tessellator.getInstance();
            WorldRenderer wr = t.getWorldRenderer();
            double rad = 0.4 + life * 3.2;
            ring(wr, 0.1, rad - 0.35, rad, 40, cr, cg, cb, 0f, alpha * 0.9f);
            ring(wr, 0.1, rad, rad + 0.12, 40, 1f, 1f, 1f, alpha, 0f);
            ring(wr, 0.9 + life, rad * 0.5 - 0.2, rad * 0.5, 32, cr, cg, cb, 0f, alpha * 0.5f);
            // Beam: two crossed vertical quads fading upward.
            double bw = 0.25 * (1 - life);
            wr.begin(GL11.GL_QUADS, DefaultVertexFormats.POSITION_COLOR);
            for (int k = 0; k < 2; k++) {
                double ax = k == 0 ? bw : 0, az = k == 0 ? 0 : bw;
                wr.pos(-ax, 0, -az).color(cr, cg, cb, alpha).endVertex();
                wr.pos(ax, 0, az).color(cr, cg, cb, alpha).endVertex();
                wr.pos(ax, 6, az).color(cr, cg, cb, 0f).endVertex();
                wr.pos(-ax, 6, -az).color(cr, cg, cb, 0f).endVertex();
            }
            t.draw();
            end();
        }

        @Override
        int lifetimeTicks() {
            return 14;
        }
    }

    /** Shooting stars streak down onto the spot and burst into sparks. */
    static final class Starfall extends ActiveEffect {
        private static final int STARS = 6;
        private final double[] sx = new double[STARS];
        private final double[] sz = new double[STARS];
        private final int[] delay = new int[STARS];

        Starfall(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
            for (int s = 0; s < STARS; s++) {
                sx[s] = RNG.nextGaussian() * 1.2;
                sz[s] = RNG.nextGaussian() * 1.2;
                delay[s] = s * 2;
            }
        }

        private static final int FALL = 6;

        @Override
        void tick(WorldClient w) {
            int step = age++;
            for (int s = 0; s < STARS; s++) {
                if (step == delay[s] + FALL) {
                    int n = allowance((int) (6 * intensity));
                    for (int k = 0; k < n; k++) {
                        w.spawnParticle(EnumParticleTypes.FIREWORKS_SPARK, x + sx[s], y + 0.2, z + sz[s],
                                RNG.nextGaussian() * 0.1, RNG.nextDouble() * 0.2, RNG.nextGaussian() * 0.1);
                    }
                }
            }
        }

        @Override
        void render(float pt, double cx, double cy, double cz) {
            float now = age + pt;
            beginGlow(x - cx, y - cy, z - cz);
            Tessellator t = Tessellator.getInstance();
            WorldRenderer wr = t.getWorldRenderer();
            GL11.glLineWidth(3.0f);
            wr.begin(GL11.GL_LINES, DefaultVertexFormats.POSITION_COLOR);
            for (int s = 0; s < STARS; s++) {
                float f = (now - delay[s]) / FALL;
                if (f < 0 || f > 1) continue;
                double hy = 9.0 * (1 - f) + 0.2;
                double hx = sx[s] + (1 - f) * 2.5;
                wr.pos(hx, hy, sz[s]).color(1f, 0.97f, 0.8f, 1f).endVertex();
                wr.pos(hx + 1.6, hy + 4.0, sz[s]).color(1f, 0.75f, 0.35f, 0f).endVertex();
            }
            t.draw();
            end();
        }

        @Override
        int lifetimeTicks() {
            return STARS * 2 + FALL + 4;
        }
    }

    /** A swirling purple portal opens around the victim, then collapses. */
    static final class Portal extends ActiveEffect {
        Portal(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            age++;
            int n = allowance(Math.max(2, (int) (6 * intensity)));
            for (int k = 0; k < n; k++) {
                w.spawnParticle(EnumParticleTypes.PORTAL, x, y + 1, z, RNG.nextGaussian() * 0.8, RNG.nextGaussian() * 0.8, RNG.nextGaussian() * 0.8);
            }
        }

        @Override
        void render(float pt, double cx, double cy, double cz) {
            float life = (age + pt) / lifetimeTicks();
            float open = life < 0.3f ? life / 0.3f : (life > 0.75f ? (1 - life) / 0.25f : 1f);
            if (open <= 0.01f) return;
            beginGlow(x - cx, y + 1.1 - cy, z - cz);
            GlStateManager.rotate((age + pt) * 18f, 0f, 1f, 0f);
            GlStateManager.rotate(90f, 1f, 0f, 0f); // a vertical ring around the body
            Tessellator t = Tessellator.getInstance();
            WorldRenderer wr = t.getWorldRenderer();
            double rad = 1.2 * open;
            ring(wr, 0, rad * 0.75, rad, 40, 0.65f, 0.25f, 1.0f, 0.2f * open, 0.9f * open);
            ring(wr, 0, 0, rad * 0.75, 40, 0.35f, 0.1f, 0.6f, 0.35f * open, 0.15f * open);
            end();
        }

        @Override
        int lifetimeTicks() {
            return 22;
        }
    }

    /** A rising funnel of wind and dust. */
    static final class Tornado extends ActiveEffect {
        Tornado(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            int per = allowance(Math.max(2, (int) (5 * intensity)));
            for (int k = 0; k < per; k++) {
                double h = (k + step * 0.4) % 3.2;
                double rr = 0.15 + h * 0.35;
                double a = step * 0.9 + h * 3 + k;
                w.spawnParticle(k % 2 == 0 ? EnumParticleTypes.CLOUD : EnumParticleTypes.SMOKE_NORMAL,
                        x + Math.cos(a) * rr, y + h, z + Math.sin(a) * rr, -Math.sin(a) * 0.12, 0.05, Math.cos(a) * 0.12);
            }
        }

        @Override
        int lifetimeTicks() {
            return 28;
        }
    }

    /** Confetti bursts upward and flutters down, spinning. */
    static final class Confetti extends ActiveEffect {
        private final Pieces p;

        Confetti(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
            int n = Math.max(10, Math.min(Math.min(b, 150), (int) (80 * i)));
            p = new Pieces(n);
            for (int k = 0; k < n; k++) {
                double a = RNG.nextDouble() * Math.PI * 2;
                double sp = 0.05 + RNG.nextDouble() * 0.15;
                p.px[k] = p.ox[k] = 0;
                p.py[k] = p.oy[k] = 1.6;
                p.pz[k] = p.oz[k] = 0;
                p.vx[k] = Math.cos(a) * sp;
                p.vy[k] = 0.3 + RNG.nextDouble() * 0.25;
                p.vz[k] = Math.sin(a) * sp;
                float[] col = hue((float) RNG.nextDouble());
                p.cr[k] = col[0];
                p.cg[k] = col[1];
                p.cb[k] = col[2];
                p.spin[k] = (float) (RNG.nextDouble() * 360);
            }
            particlesSpawned = n;
        }

        @Override
        void tick(WorldClient w) {
            age++;
            p.step(0.025, 0.9); // strong drag: paper flutters down slowly
        }

        @Override
        void render(float pt, double cx, double cy, double cz) {
            float life = (age + pt) / lifetimeTicks();
            float alpha = life < 0.8f ? 1f : (1 - life) / 0.2f;
            if (alpha <= 0.01f) return;
            beginSolid(x - cx, y - cy, z - cz);
            Tessellator t = Tessellator.getInstance();
            WorldRenderer wr = t.getWorldRenderer();
            wr.begin(GL11.GL_QUADS, DefaultVertexFormats.POSITION_COLOR);
            float time = age + pt;
            for (int k = 0; k < p.n; k++) {
                double px = p.ix(k, pt), py = p.iy(k, pt), pz = p.iz(k, pt);
                double a = Math.toRadians(p.spin[k] + time * 25);
                double s = 0.07;
                double ux = Math.cos(a) * s, uz = Math.sin(a) * s;
                double vy = Math.sin(a * 1.7) * s;
                quad(wr, px - ux, py - vy, pz - uz, px + ux, py - vy, pz + uz, px + ux, py + s, pz + uz, px - ux, py + s, pz - uz,
                        p.cr[k], p.cg[k], p.cb[k], alpha);
            }
            t.draw();
            end();
        }

        @Override
        int lifetimeTicks() {
            return 50;
        }
    }

    /** Kestrel strike: orange wings sweep open behind a glowing chevron. */
    static final class Kestrel extends ActiveEffect {
        Kestrel(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            double open = Math.min(1.0, step / 8.0);
            int per = allowance(Math.max(2, (int) (5 * intensity)));
            for (int k = 0; k < per; k++) {
                double t = k / (double) Math.max(1, per - 1);
                double span = 0.3 + t * 2.0 * open;
                double lift = 1.3 + (1 - t) * 0.4 - t * t * 0.9;
                for (int side = -1; side <= 1; side += 2) {
                    boolean flame = k % 3 == 0;
                    w.spawnParticle(flame ? EnumParticleTypes.FLAME : EnumParticleTypes.REDSTONE,
                            x + side * span, y + lift, z, flame ? 0 : 0.91, flame ? 0.01 : 0.51, flame ? 0 : 0.23);
                }
            }
        }

        @Override
        void render(float pt, double cx, double cy, double cz) {
            float life = (age + pt) / lifetimeTicks();
            float a = life < 0.2f ? life / 0.2f : (1 - life) / 0.8f;
            if (a <= 0.01f) return;
            beginGlow(x - cx, y + 2.4 + life * 0.6 - cy, z - cz);
            // Face the chevron toward the viewer around the vertical axis.
            GlStateManager.rotate((float) Math.toDegrees(Math.atan2(x - cx, z - cz)) + 180f, 0f, 1f, 0f);
            Tessellator t = Tessellator.getInstance();
            WorldRenderer wr = t.getWorldRenderer();
            wr.begin(GL11.GL_QUADS, DefaultVertexFormats.POSITION_COLOR);
            // Two strokes of a chevron: \/ shape with thickness.
            float cr = 0.91f, cg = 0.51f, cb = 0.23f;
            quad(wr, -0.6, 0.3, 0, -0.45, 0.3, 0, 0.0, -0.1, 0, 0.0, 0.05, 0, cr, cg, cb, a);
            quad(wr, 0.0, 0.05, 0, 0.0, -0.1, 0, 0.45, 0.3, 0, 0.6, 0.3, 0, cr, cg, cb, a);
            t.draw();
            end();
        }

        @Override
        int lifetimeTicks() {
            return 20;
        }
    }
}
