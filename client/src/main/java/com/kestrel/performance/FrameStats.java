package com.kestrel.performance;

import java.util.Arrays;

/**
 * Rolling frame-time statistics: average FPS and 1% low over the last
 * {@code capacity} frames. Recording is O(1) and allocation-free; statistics
 * are recomputed on demand at most every {@link #RECOMPUTE_MS}.
 */
public final class FrameStats {
    static final long RECOMPUTE_MS = 500;

    private final long[] frameNanos;
    private final long[] scratch;
    private int count;
    private int next;
    private long lastFrameNs;

    private long lastComputeMs;
    private int avgFps;
    private int onePercentLowFps;
    private float maxFrameMs;

    public FrameStats(int capacity) {
        frameNanos = new long[capacity];
        scratch = new long[capacity];
    }

    /** Record the end of a frame at time {@code nowNs} (System.nanoTime()). */
    public void frame(long nowNs) {
        if (lastFrameNs != 0) {
            long dt = nowNs - lastFrameNs;
            // Ignore pauses (alt-tab, world load) longer than 1 s.
            if (dt > 0 && dt < 1_000_000_000L) {
                frameNanos[next] = dt;
                next = (next + 1) % frameNanos.length;
                if (count < frameNanos.length) count++;
            }
        }
        lastFrameNs = nowNs;
    }

    /** Recompute if stale; cheap otherwise. */
    public void update(long nowMs) {
        if (nowMs - lastComputeMs < RECOMPUTE_MS) return;
        lastComputeMs = nowMs;
        compute();
    }

    void compute() {
        if (count == 0) {
            avgFps = 0;
            onePercentLowFps = 0;
            maxFrameMs = 0;
            return;
        }
        long total = 0;
        for (int i = 0; i < count; i++) {
            scratch[i] = frameNanos[i];
            total += frameNanos[i];
        }
        Arrays.sort(scratch, 0, count);
        avgFps = (int) Math.round(1e9 * count / (double) total);
        // 1% low: average FPS over the slowest 1% of frames (at least one).
        int n = Math.max(1, count / 100);
        long slow = 0;
        for (int i = count - n; i < count; i++) slow += scratch[i];
        onePercentLowFps = (int) Math.round(1e9 * n / (double) slow);
        maxFrameMs = scratch[count - 1] / 1e6f;
    }

    public void reset() {
        count = 0;
        next = 0;
        lastFrameNs = 0;
    }

    public int avgFps() {
        return avgFps;
    }

    public int onePercentLowFps() {
        return onePercentLowFps;
    }

    public float maxFrameMs() {
        return maxFrameMs;
    }

    public int samples() {
        return count;
    }
}
