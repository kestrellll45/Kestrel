package com.kestrel.input;

/**
 * Clicks-per-second over a sliding 1-second window. Fed from Forge's
 * in-game MouseEvent only (no global input hooks). Allocation-free.
 */
public final class CpsTracker {
    private static final int CAPACITY = 64; // > any humanly possible CPS
    private static final long WINDOW_MS = 1000;

    private final long[] times = new long[CAPACITY];
    private int head;
    private int size;

    public void click(long nowMs) {
        times[head] = nowMs;
        head = (head + 1) % CAPACITY;
        if (size < CAPACITY) size++;
    }

    public int cps(long nowMs) {
        int n = 0;
        for (int i = 0; i < size; i++) {
            int idx = (head - 1 - i + CAPACITY) % CAPACITY;
            if (nowMs - times[idx] < WINDOW_MS) {
                n++;
            } else {
                break; // entries are chronological; older ones are out of window too
            }
        }
        return n;
    }

    public void clear() {
        size = 0;
    }
}
