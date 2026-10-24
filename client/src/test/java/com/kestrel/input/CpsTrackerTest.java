package com.kestrel.input;

import org.junit.Test;

import static org.junit.Assert.assertEquals;

public class CpsTrackerTest {
    @Test
    public void slidingWindow() {
        CpsTracker c = new CpsTracker();
        for (int i = 0; i < 10; i++) c.click(1000 + i * 50);
        assertEquals(10, c.cps(1500));
        // Window is (now - 1000 ms, now]: clicks at 1300..1450 count, 1250 is exactly 1 s old.
        assertEquals(4, c.cps(2250));
        assertEquals(5, c.cps(2249));
        assertEquals(0, c.cps(5000));
    }

    @Test
    public void boundedCapacity() {
        CpsTracker c = new CpsTracker();
        for (int i = 0; i < 500; i++) c.click(1000);
        assertEquals(64, c.cps(1000));
    }
}
