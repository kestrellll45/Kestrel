package com.kestrel.cosmetics;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.fail;

public class FrameTimelineTest {
    @Test
    public void perFrameDelaysAreRespected() {
        FrameTimeline t = FrameTimeline.ofDelays(new int[]{70, 60, 100});
        assertEquals(230, t.loopMs());
        assertEquals(0, t.frameAt(0, 10));
        assertEquals(0, t.frameAt(69, 10));
        assertEquals(1, t.frameAt(70, 10));
        assertEquals(1, t.frameAt(129, 10));
        assertEquals(2, t.frameAt(130, 10));
        assertEquals(2, t.frameAt(229, 10));
        assertEquals(0, t.frameAt(230, 10)); // loops
        assertEquals(2, t.frameAt(-1, 10));  // negative time wraps too
        assertEquals(1, t.frameAt(230L * 1_000_000 + 75, 10));
    }

    @Test
    public void fixedRateUsesFps() {
        FrameTimeline t = FrameTimeline.fixed(4);
        assertEquals(0, t.frameAt(99, 10));
        assertEquals(1, t.frameAt(100, 10));
        assertEquals(0, t.frameAt(400, 10));
        assertEquals(0, t.frameAt(5, 0)); // fps clamped, no division by zero
    }

    @Test
    public void singleFrameIsAlwaysZero() {
        assertEquals(0, FrameTimeline.ofDelays(new int[]{50}).frameAt(12345, 10));
        assertEquals(0, FrameTimeline.fixed(1).frameAt(12345, 10));
    }

    @Test
    public void rejectsBadInput() {
        try {
            FrameTimeline.ofDelays(new int[0]);
            fail();
        } catch (IllegalArgumentException expected) {
        }
        try {
            FrameTimeline.ofDelays(new int[]{10, 0});
            fail();
        } catch (IllegalArgumentException expected) {
        }
    }
}
