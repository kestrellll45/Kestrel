package com.kestrel.effects;

import net.minecraft.client.multiplayer.WorldClient;
import net.minecraft.client.renderer.GlStateManager;
import net.minecraft.client.renderer.Tessellator;
import net.minecraft.client.renderer.WorldRenderer;
import net.minecraft.client.renderer.vertex.DefaultVertexFormats;
import net.minecraft.util.EnumParticleTypes;
import org.lwjgl.opengl.GL11;

import java.util.Random;

/**
 * Effect implementations. Particles go through the vanilla particle system
 * (so the player's Particles video setting still applies) and every effect
 * is capped by its particle budget.
 */
final class Effects {
    private static final Random RNG = new Random();

    private Effects() {
    }

    static ActiveEffect create(KillEffectType type, double x, double y, double z, float intensity, int budget, int color) {
        switch (type) {
            case RING:
                return new Ring(x, y, z, intensity, budget);
            case HELIX:
                return new Helix(x, y, z, intensity, budget, color);
            case DUST:
                return new DustBurst(x, y, z, intensity, budget, color);
            case HEARTS:
                return new Hearts(x, y, z, intensity, budget);
            case FLAMES:
                return new Flames(x, y, z, intensity, budget);
            case ARCANE:
                return new Arcane(x, y, z, intensity, budget);
            case EXPLOSION:
                return new Explosion(x, y, z, intensity, budget);
            case LIGHTNING:
                return new Lightning(x, y, z, intensity, budget);
            case BURST:
            default:
                return new Burst(x, y, z, intensity, budget);
        }
    }

    /** One-tick burst of sparks. */
    static final class Burst extends ActiveEffect {
        Burst(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            if (age++ != 0) return;
            int n = allowance((int) (40 * intensity));
            for (int k = 0; k < n; k++) {
                double vx = RNG.nextGaussian() * 0.15;
                double vy = RNG.nextDouble() * 0.3 + 0.05;
                double vz = RNG.nextGaussian() * 0.15;
                w.spawnParticle(k % 3 == 0 ? EnumParticleTypes.CRIT_MAGIC : EnumParticleTypes.FIREWORKS_SPARK, x, y + 1.0, z, vx, vy, vz);
            }
        }

        @Override
        int lifetimeTicks() {
            return 1;
        }
    }

    /** Expanding rising ring over ten ticks. */
    static final class Ring extends ActiveEffect {
        Ring(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            int per = allowance((int) (10 * intensity));
            double r = 0.4 + step * 0.12;
            double yy = y + 0.1 + step * 0.18;
            for (int k = 0; k < per; k++) {
                double a = 2 * Math.PI * k / Math.max(1, per) + step * 0.3;
                w.spawnParticle(EnumParticleTypes.SPELL_WITCH, x + Math.cos(a) * r, yy, z + Math.sin(a) * r, 0, 0, 0);
            }
        }

        @Override
        int lifetimeTicks() {
            return 10;
        }
    }

    /** Redstone dust colour components; dust treats red == 0 as "default red". */
    static float red(int argb) {
        return Math.max(0.001f, (argb >> 16 & 255) / 255f);
    }

    static float green(int argb) {
        return (argb >> 8 & 255) / 255f;
    }

    static float blue(int argb) {
        return (argb & 255) / 255f;
    }

    /** Two rising dust spirals: your colour and white ("coloured trail"). */
    static final class Helix extends ActiveEffect {
        private final int color;

        Helix(double x, double y, double z, float i, int b, int color) {
            super(x, y, z, i, b);
            this.color = color;
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            int per = allowance(Math.max(2, (int) (4 * intensity)));
            for (int k = 0; k < per; k++) {
                double t = step + k / (double) per;
                double a = t * 0.9 + (k % 2) * Math.PI;
                double yy = y + t * 0.12;
                // Redstone dust reads its colour from the velocity arguments.
                boolean mine = k % 2 == 0;
                w.spawnParticle(EnumParticleTypes.REDSTONE, x + Math.cos(a) * 0.6, yy, z + Math.sin(a) * 0.6,
                        mine ? red(color) : 1.0, mine ? green(color) : 1.0, mine ? blue(color) : 1.0);
            }
        }

        @Override
        int lifetimeTicks() {
            return 18;
        }
    }

    /** Sphere of dust in your colour, expanding over a few ticks. */
    static final class DustBurst extends ActiveEffect {
        private final int color;

        DustBurst(double x, double y, double z, float i, int b, int color) {
            super(x, y, z, i, b);
            this.color = color;
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            int per = allowance((int) (14 * intensity));
            double r = 0.3 + step * 0.35;
            for (int k = 0; k < per; k++) {
                // Uniform points on a sphere.
                double u = RNG.nextDouble() * 2 - 1;
                double t = RNG.nextDouble() * Math.PI * 2;
                double s = Math.sqrt(1 - u * u);
                w.spawnParticle(EnumParticleTypes.REDSTONE, x + s * Math.cos(t) * r, y + 1.0 + u * r, z + s * Math.sin(t) * r,
                        red(color), green(color), blue(color));
            }
        }

        @Override
        int lifetimeTicks() {
            return 4;
        }
    }

    /** Fountain of hearts. */
    static final class Hearts extends ActiveEffect {
        Hearts(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            age++;
            int n = allowance(Math.max(1, (int) (3 * intensity)));
            for (int k = 0; k < n; k++) {
                w.spawnParticle(EnumParticleTypes.HEART, x + RNG.nextGaussian() * 0.4, y + 1.2 + RNG.nextDouble() * 0.8,
                        z + RNG.nextGaussian() * 0.4, 0, 0.1, 0);
            }
        }

        @Override
        int lifetimeTicks() {
            return 8;
        }
    }

    /** Spiralling column of flames. */
    static final class Flames extends ActiveEffect {
        Flames(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            int step = age++;
            int per = allowance(Math.max(2, (int) (6 * intensity)));
            for (int k = 0; k < per; k++) {
                double a = step * 0.7 + k * (2 * Math.PI / per);
                double r = 0.5 - step * 0.02;
                w.spawnParticle(EnumParticleTypes.FLAME, x + Math.cos(a) * r, y + step * 0.15, z + Math.sin(a) * r, 0, 0.02, 0);
            }
        }

        @Override
        int lifetimeTicks() {
            return 16;
        }
    }

    /** Enchanting glyphs that fly into the victim (vanilla glyphs move toward their origin). */
    static final class Arcane extends ActiveEffect {
        Arcane(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            age++;
            int n = allowance((int) (10 * intensity));
            for (int k = 0; k < n; k++) {
                w.spawnParticle(EnumParticleTypes.ENCHANTMENT_TABLE, x, y + 1.2, z,
                        RNG.nextGaussian() * 1.5, RNG.nextDouble() * 1.5, RNG.nextGaussian() * 1.5);
            }
            if (age == 1) {
                int s = allowance((int) (8 * intensity));
                for (int k = 0; k < s; k++) {
                    w.spawnParticle(EnumParticleTypes.SPELL_WITCH, x, y + 1, z, 0, 0, 0);
                }
            }
        }

        @Override
        int lifetimeTicks() {
            return 6;
        }
    }

    /** Explosion particles (no block damage, no knockback: purely visual). */
    static final class Explosion extends ActiveEffect {
        Explosion(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
        }

        @Override
        void tick(WorldClient w) {
            if (age++ != 0) return;
            int big = allowance(Math.max(1, (int) (3 * intensity)));
            for (int k = 0; k < big; k++) {
                w.spawnParticle(EnumParticleTypes.EXPLOSION_LARGE, x + RNG.nextGaussian() * 0.4, y + 1 + RNG.nextGaussian() * 0.3, z + RNG.nextGaussian() * 0.4, 0, 0, 0);
            }
            int smoke = allowance((int) (20 * intensity));
            for (int k = 0; k < smoke; k++) {
                w.spawnParticle(EnumParticleTypes.SMOKE_LARGE, x, y + 0.8, z, RNG.nextGaussian() * 0.08, 0.05 + RNG.nextDouble() * 0.08, RNG.nextGaussian() * 0.08);
            }
        }

        @Override
        int lifetimeTicks() {
            return 1;
        }
    }

    /**
     * Client-drawn lightning bolt (line strip, additive blend). Drawn
     * ourselves rather than spawning a vanilla lightning entity so it never
     * lights up the world or plays thunder unless sounds are enabled.
     */
    static final class Lightning extends ActiveEffect {
        private static final int SEGMENTS = 14;
        private final double[] px = new double[SEGMENTS + 1];
        private final double[] pz = new double[SEGMENTS + 1];

        Lightning(double x, double y, double z, float i, int b) {
            super(x, y, z, i, b);
            for (int s = 1; s < SEGMENTS; s++) {
                px[s] = px[s - 1] * 0.5 + RNG.nextGaussian() * 0.6;
                pz[s] = pz[s - 1] * 0.5 + RNG.nextGaussian() * 0.6;
            }
        }

        @Override
        void tick(WorldClient w) {
            if (age == 0) {
                int n = allowance((int) (12 * intensity));
                for (int k = 0; k < n; k++) {
                    w.spawnParticle(EnumParticleTypes.FIREWORKS_SPARK, x, y + 0.2, z, RNG.nextGaussian() * 0.1, RNG.nextDouble() * 0.2, RNG.nextGaussian() * 0.1);
                }
            }
            age++;
        }

        @Override
        void render(float pt, double cx, double cy, double cz) {
            float life = (age + pt) / lifetimeTicks();
            float alpha = Math.max(0f, 1f - life) * Math.min(1f, intensity);
            if (alpha <= 0.01f) return;
            double top = 18.0;
            Tessellator t = Tessellator.getInstance();
            WorldRenderer wr = t.getWorldRenderer();
            GlStateManager.pushMatrix();
            GlStateManager.translate(x - cx, y - cy, z - cz);
            GlStateManager.disableTexture2D();
            GlStateManager.disableLighting();
            GlStateManager.enableBlend();
            GlStateManager.blendFunc(GL11.GL_SRC_ALPHA, GL11.GL_ONE);
            GlStateManager.depthMask(false);
            for (int pass = 0; pass < 2; pass++) {
                GL11.glLineWidth(pass == 0 ? 6.0f : 2.0f);
                float a = pass == 0 ? alpha * 0.35f : alpha;
                wr.begin(GL11.GL_LINE_STRIP, DefaultVertexFormats.POSITION_COLOR);
                for (int s = 0; s <= SEGMENTS; s++) {
                    double yy = top - top * s / SEGMENTS;
                    wr.pos(px[s], yy, pz[s]).color(0.75f, 0.85f, 1.0f, a).endVertex();
                }
                t.draw();
            }
            GL11.glLineWidth(1.0f);
            GlStateManager.depthMask(true);
            GlStateManager.blendFunc(GL11.GL_SRC_ALPHA, GL11.GL_ONE_MINUS_SRC_ALPHA);
            GlStateManager.disableBlend();
            GlStateManager.enableTexture2D();
            GlStateManager.popMatrix();
        }

        @Override
        int lifetimeTicks() {
            return 12;
        }
    }
}
