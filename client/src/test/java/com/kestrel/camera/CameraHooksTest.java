package com.kestrel.camera;

import org.junit.After;
import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class CameraHooksTest {
    @After
    public void clear() {
        CameraHooks.publish(1f, null, 0f, 0f);
        CameraHooks.setTurnHandler(null);
    }

    @Test
    public void onlyTheOverriddenEntityGetsCameraAngles() {
        Object player = new Object();
        Object other = new Object();
        CameraHooks.publish(0.5f, player, 123f, -45f);
        assertEquals(123f, CameraHooks.yaw(player, 1f), 0f);
        assertEquals(-45f, CameraHooks.pitch(player, 1f), 0f);
        assertEquals(1f, CameraHooks.yaw(other, 1f), 0f);
        assertEquals(2f, CameraHooks.pitch(null, 2f), 0f);
        assertEquals(0.5f, CameraHooks.fovMultiplier(), 0f);
        CameraHooks.publish(1f, null, 123f, -45f);
        assertEquals(7f, CameraHooks.yaw(player, 7f), 0f);
        assertFalse(CameraHooks.overrides(player));
    }

    @Test
    public void invalidZoomMultiplierIsIgnored() {
        CameraHooks.publish(Float.NaN, null, 0f, 0f);
        assertEquals(1f, CameraHooks.fovMultiplier(), 0f);
        CameraHooks.publish(2f, null, 0f, 0f);
        assertEquals(1f, CameraHooks.fovMultiplier(), 0f);
    }

    @Test
    public void turnHandlerIsNotReentrant() {
        final int[] calls = {0};
        CameraHooks.setTurnHandler(new CameraHooks.TurnHandler() {
            @Override
            public boolean turn(Object entity, float yaw, float pitch) {
                calls[0]++;
                // A nested setAngles (scaled turn) must run vanilla.
                assertFalse(CameraHooks.onSetAngles(entity, yaw / 2, pitch / 2));
                return true;
            }
        });
        assertTrue(CameraHooks.onSetAngles(new Object(), 4f, 2f));
        assertEquals(1, calls[0]);
        CameraHooks.setTurnHandler(null);
        assertFalse(CameraHooks.onSetAngles(new Object(), 4f, 2f));
    }
}
