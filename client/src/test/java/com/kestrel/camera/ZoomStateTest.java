package com.kestrel.camera;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class ZoomStateTest {
    private static final float FRAME = 1f / 60f;

    @Test
    public void startsUnzoomed() {
        ZoomState z = new ZoomState();
        assertEquals(1f, z.fovMultiplier(), 0f);
        assertFalse(z.isZoomed());
    }

    @Test
    public void smoothlyApproachesTargetAndBack() {
        ZoomState z = new ZoomState();
        float prev = 1f;
        for (int i = 0; i < 120; i++) {
            z.update(true, 4f, 14f, true, FRAME);
            assertTrue("monotonic zoom-in", z.fovMultiplier() <= prev);
            prev = z.fovMultiplier();
        }
        assertEquals(0.25f, z.fovMultiplier(), 0f); // snapped exactly to 1/factor
        z.update(false, 4f, 14f, true, FRAME);
        assertTrue(z.fovMultiplier() > 0.25f && z.fovMultiplier() < 1f);
        for (int i = 0; i < 120; i++) z.update(false, 4f, 14f, true, FRAME);
        assertEquals(1f, z.fovMultiplier(), 0f);
        assertFalse(z.isZoomed());
    }

    @Test
    public void neverStacksOrOvershoots() {
        ZoomState z = new ZoomState();
        for (int i = 0; i < 10_000; i++) {
            z.update(i % 7 < 4, 10f, 40f, true, (i % 13) / 100f);
            assertTrue(z.fovMultiplier() >= 0.1f - 1e-6f && z.fovMultiplier() <= 1f);
        }
    }

    @Test
    public void instantWhenNotSmooth() {
        ZoomState z = new ZoomState();
        z.update(true, 5f, 14f, false, 0f);
        assertEquals(0.2f, z.fovMultiplier(), 1e-6f);
        z.update(false, 5f, 14f, false, 0f);
        assertEquals(1f, z.fovMultiplier(), 0f);
    }

    @Test
    public void longStallsAndBadInputAreSafe() {
        ZoomState z = new ZoomState();
        z.update(true, 4f, 14f, true, 30f); // a 30 s hitch counts as 0.25 s
        assertTrue(z.fovMultiplier() > 0.25f);
        z.update(true, Float.NaN, 14f, true, FRAME);
        z.update(true, 0.5f, 14f, true, Float.NaN);
        assertTrue(z.fovMultiplier() > 0f && z.fovMultiplier() <= 1f);
        z.reset();
        assertEquals(1f, z.fovMultiplier(), 0f);
    }
}
