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
        f.update(FRAME, player, 20f, FreeLookState.RETURN_DURATION);
        assertTrue("moves forward across 180, not back through 0", f.yaw() > prev);
        int frames = 0;
        while (f.overrides() && frames++ < 600) {
            f.update(FRAME, player, 20f, FreeLookState.RETURN_DURATION);
        }
        assertFalse(f.overrides());
        assertEquals(0f, FreeLookState.wrapDegrees(player - f.yaw()), 0f);
        assertEquals(20f, f.pitch(), 0f);
        assertTrue("return finishes within its fixed duration", frames <= Math.ceil(FreeLookState.RETURN_DURATION / FRAME));
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
        f.update(FRAME, 0f, 0f, FreeLookState.RETURN_DURATION);
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

    @Test
    public void returnStartsOnTheReleaseFrameAndIsBoundedForAnyAngle() {
        for (float turned : new float[] {5f, 90f, 179f, 720f}) {
            FreeLookState f = new FreeLookState();
            f.press(0f, 0f);
            f.turn(turned / 0.15f, 300f);
            float startYaw = f.yaw();
            float startPitch = f.pitch();
            f.release(true);
            f.update(FRAME, 0f, 0f, FreeLookState.RETURN_DURATION);
            assertTrue("moves on the first frame", f.yaw() != startYaw && f.pitch() != startPitch
                    || FreeLookState.wrapDegrees(startYaw) == 0f && f.pitch() != startPitch);
            int frames = 1;
            while (f.overrides()) {
                f.update(FRAME, 0f, 0f, FreeLookState.RETURN_DURATION);
                frames++;
                assertTrue("bounded return", frames <= 10);
            }
            assertEquals(0f, FreeLookState.wrapDegrees(f.yaw()), 0f);
            assertEquals(0f, f.pitch(), 0f);
        }
    }

    @Test
    public void returnFollowsThePlayerIfTheyTurnMeanwhile() {
        FreeLookState f = new FreeLookState();
        f.press(0f, 0f);
        f.turn(600f, 0f);
        f.release(true);
        float player = 0f;
        while (f.overrides()) {
            player += 1f; // mouse now turns the player
            f.update(FRAME, player, 5f, FreeLookState.RETURN_DURATION);
        }
        assertEquals(player, f.yaw(), 0f);
        assertEquals(5f, f.pitch(), 0f);
    }

    @Test
    public void easeOutStartsFast() {
        assertTrue(FreeLookState.easeOutCubic(0.1f) > 0.25);
        assertEquals(1.0, FreeLookState.easeOutCubic(1f), 0.0);
        assertEquals(0.0, FreeLookState.easeOutCubic(0f), 0.0);
    }
}
