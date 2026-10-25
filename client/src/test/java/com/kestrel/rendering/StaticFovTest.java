package com.kestrel.rendering;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

public class StaticFovTest {
    /** Vanilla 1.8.9 world FOV (EntityRenderer.getFOVModifier with useFOVSetting = true), for contrast. */
    private static float vanilla(float fovSetting, float fovModifierHand, boolean dying, boolean inWater) {
        float f = fovSetting * fovModifierHand;
        if (dying) {
            float t = 10.0f; // deathTime + partialTicks
            f /= (1.0f - 500.0f / (t + 500.0f)) * 2.0f + 1.0f;
        }
        if (inWater) {
            f = f * 60.0f / 70.0f;
        }
        return f;
    }

    @Test
    public void worldViewIsAlwaysTheConfiguredFov() {
        // Modifiers vanilla would apply: sprint (~1.15), Speed II (~1.2), Slowness (~0.85),
        // flying (1.1), full bow draw (0.85), plus dying and being under water.
        float[] handModifiers = {1.0f, 1.15f, 1.2f, 0.85f, 1.1f, 0.85f};
        for (float configured : new float[] {30.0f, 70.0f, 85.0f, 110.0f}) {
            for (float m : handModifiers) {
                for (boolean dying : new boolean[] {false, true}) {
                    for (boolean water : new boolean[] {false, true}) {
                        boolean vanillaChanges = vanilla(configured, m, dying, water) != configured;
                        if (m != 1.0f || dying || water) {
                            assertTrue("test case must be a real vanilla FOV change", vanillaChanges);
                        }
                        // Kestrel ignores all of them: only the setting matters.
                        assertEquals(configured, StaticFov.fov(false, true, configured), 0.0f);
                    }
                }
            }
        }
    }

    @Test
    public void configuredFovStaysAdjustable() {
        assertEquals(30.0f, StaticFov.fov(false, true, 30.0f), 0.0f);
        assertEquals(110.0f, StaticFov.fov(false, true, 110.0f), 0.0f);
        assertEquals(82.5f, StaticFov.fov(false, true, 82.5f), 0.0f);
    }

    @Test
    public void heldItemViewKeepsVanillaFixedFov() {
        assertEquals(70.0f, StaticFov.fov(false, false, 110.0f), 0.0f);
        assertEquals(70.0f, StaticFov.fov(false, false, 30.0f), 0.0f);
    }

    @Test
    public void debugCameraUnchanged() {
        assertEquals(90.0f, StaticFov.fov(true, true, 70.0f), 0.0f);
        assertEquals(90.0f, StaticFov.fov(true, false, 70.0f), 0.0f);
    }

    @Test
    public void sameInputSameOutputEveryFrame() {
        // No state: repeated calls cannot drift, flicker or accumulate.
        float first = StaticFov.fov(false, true, 95.0f);
        for (int i = 0; i < 1000; i++) {
            assertEquals(first, StaticFov.fov(false, true, 95.0f), 0.0f);
        }
    }
}
