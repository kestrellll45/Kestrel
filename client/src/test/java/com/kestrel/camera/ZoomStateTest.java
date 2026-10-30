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

    @Test
    public void easesInAndOutWithinTheConfiguredTime() {
        ZoomState z = new ZoomState();
        float speed = 14f; // ~0.214 s
        float d = ZoomState.duration(speed);
        int frames = (int) Math.ceil(d / FRAME);
        float[] m = new float[frames + 1];
        m[0] = 1f;
        for (int i = 1; i <= frames; i++) {
            z.update(true, 4f, speed, true, FRAME);
            m[i] = z.fovMultiplier();
        }
        assertEquals("finished on time", 0.25f, m[frames], 0f);
        assertFalse(z.isAnimating());
        // Starts and ends gently (smaller steps than in the middle), never jumps.
        double first = Math.log(m[0]) - Math.log(m[1]);
        double last = Math.log(m[frames - 1]) - Math.log(m[frames]);
        double mid = Math.log(m[frames / 2 - 1]) - Math.log(m[frames / 2]);
        assertTrue(first < mid && last < mid);
        assertTrue("first frame already moves", first > 0);
        assertTrue("no step bigger than 15% of the zoom range", mid < Math.log(4) * 0.15);
    }

    @Test
    public void releasingMidZoomReversesFromTheCurrentValue() {
        ZoomState z = new ZoomState();
        for (int i = 0; i < 5; i++) z.update(true, 4f, 14f, true, FRAME);
        float mid = z.fovMultiplier();
        assertTrue(mid < 1f && mid > 0.25f);
        z.update(false, 4f, 14f, true, FRAME);
        assertTrue("no jump on reversal", Math.abs(z.fovMultiplier() - mid) < 0.02f);
        assertTrue(z.fovMultiplier() >= mid);
    }

    @Test
    public void durationFollowsSpeedSetting() {
        assertTrue(ZoomState.duration(2f) > ZoomState.duration(14f));
        assertTrue(ZoomState.duration(14f) > ZoomState.duration(40f));
        assertEquals(ZoomState.MAX_DURATION, ZoomState.duration(0.1f), 0f);
        assertEquals(ZoomState.MIN_DURATION, ZoomState.duration(1000f), 0f);
    }
}
