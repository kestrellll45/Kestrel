package com.kestrel.camera;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class FreeLookStateTest {
    private static final float FRAME = 1f / 60f;

    @Test
    public void startsFromPlayerViewAndTurnsIndependently() {
        FreeLookState f = new FreeLookState();
        assertFalse(f.overrides());
        f.press(30f, 10f);
        assertTrue(f.overrides());
        assertEquals(30f, f.yaw(), 0f);
        assertEquals(10f, f.pitch(), 0f);
        // Full 360 degrees horizontally (2400 mouse units * 0.15 = 360).
        f.turn(2400f, 0f);
        assertEquals(390f, f.yaw(), 1e-3f);
        // Pitch is clamped like vanilla.
        f.turn(0f, -10_000f);
        assertEquals(90f, f.pitch(), 0f);
        f.turn(0f, 10_000f);
        assertEquals(-90f, f.pitch(), 0f);
    }

    @Test
    public void smoothReturnTakesShortestWayAndEnds() {
        FreeLookState f = new FreeLookState();
        f.press(0f, 0f);
        f.turn(170f / 0.15f, 0f); // camera now at 170 degrees
        f.release(true);
        assertTrue(f.isReturning());
        float player = -170f; // shortest way from 170 to -170 is +20 degrees
        float prev = f.yaw();
        f.update(FRAME, player, 20f, FreeLookState.RETURN_SPEED);
        assertTrue("moves forward across 180, not back through 0", f.yaw() > prev);
        int frames = 0;
        while (f.overrides() && frames++ < 600) {
            f.update(FRAME, player, 20f, FreeLookState.RETURN_SPEED);
        }
        assertFalse(f.overrides());
        assertEquals(0f, FreeLookState.wrapDegrees(player - f.yaw()), FreeLookState.DONE);
        assertEquals(20f, f.pitch(), FreeLookState.DONE);
        assertTrue("return finishes in well under a second", frames < 60);
    }

    @Test
    public void instantReturnAndReset() {
        FreeLookState f = new FreeLookState();
        f.press(0f, 0f);
        f.release(false);
        assertFalse(f.overrides());
        f.press(0f, 0f);
        f.reset();
        assertFalse(f.overrides());
        f.release(true); // releasing when not held does nothing
        assertFalse(f.overrides());
    }

    @Test
    public void repressDuringReturnContinuesFromCurrentCamera() {
        FreeLookState f = new FreeLookState();
        f.press(0f, 0f);
        f.turn(600f, 0f); // 90 degrees
        f.release(true);
        f.update(FRAME, 0f, 0f, FreeLookState.RETURN_SPEED);
        float mid = f.yaw();
        f.press(0f, 0f);
        assertEquals("no jump when pressed again mid-return", mid, f.yaw(), 0f);
        assertTrue(f.isHeld());
    }

    @Test
    public void wrapDegrees() {
        assertEquals(0f, FreeLookState.wrapDegrees(360f), 0f);
        assertEquals(-180f, FreeLookState.wrapDegrees(180f), 0f);
        assertEquals(170f, FreeLookState.wrapDegrees(-190f), 0f);
        assertEquals(-10f, FreeLookState.wrapDegrees(710f), 1e-4f);
    }
}
