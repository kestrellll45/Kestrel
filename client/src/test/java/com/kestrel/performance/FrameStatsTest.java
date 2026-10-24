package com.kestrel.performance;

import org.junit.Test;

import static org.junit.Assert.assertEquals;

public class FrameStatsTest {
    @Test
    public void averageAndOnePercentLow() {
        FrameStats s = new FrameStats(1000);
        long t = 1_000_000_000L;
        s.frame(t);
        // 990 frames at 4 ms (250 FPS) + 10 frames at 20 ms (50 FPS).
        for (int i = 0; i < 990; i++) s.frame(t += 4_000_000L);
        for (int i = 0; i < 10; i++) s.frame(t += 20_000_000L);
        s.compute();
        assertEquals(1000, s.samples());
        assertEquals(50, s.onePercentLowFps());
        assertEquals(1e9 * 1000 / (990 * 4e6 + 10 * 20e6), s.avgFps(), 1.0);
        assertEquals(20.0f, s.maxFrameMs(), 0.01f);
    }

    @Test
    public void ignoresLongPausesAndWraps() {
        FrameStats s = new FrameStats(10);
        long t = 5_000_000_000L;
        s.frame(t);
        s.frame(t += 5_000_000_000L); // 5 s pause ignored
        for (int i = 0; i < 25; i++) s.frame(t += 10_000_000L);
        s.compute();
        assertEquals(10, s.samples());
        assertEquals(100, s.avgFps());
    }
}
