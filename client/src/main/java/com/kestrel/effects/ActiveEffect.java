package com.kestrel.effects;

import net.minecraft.client.multiplayer.WorldClient;

/** A running visual effect at a world position. */
abstract class ActiveEffect {
    final double x;
    final double y;
    final double z;
    final float intensity;
    final int particleBudget;
    int age;
    int particlesSpawned;

    ActiveEffect(double x, double y, double z, float intensity, int particleBudget) {
        this.x = x;
        this.y = y;
        this.z = z;
        this.intensity = intensity;
        this.particleBudget = particleBudget;
    }

    /** Called once per client tick. */
    abstract void tick(WorldClient world);

    /** Called from RenderWorldLastEvent for effects with custom geometry. */
    void render(float partialTicks, double camX, double camY, double camZ) {
    }

    abstract int lifetimeTicks();

    final boolean finished() {
        return age >= lifetimeTicks();
    }

    /** How many particles we may still spawn this tick (never exceeds the budget). */
    final int allowance(int wanted) {
        int left = particleBudget - particlesSpawned;
        int n = Math.max(0, Math.min(left, wanted));
        particlesSpawned += n;
        return n;
    }
}
