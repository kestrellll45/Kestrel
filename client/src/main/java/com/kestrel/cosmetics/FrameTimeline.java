package com.kestrel.cosmetics;

/**
 * Maps wall-clock time to an animation frame using per-frame durations
 * (GIF delays) or a fixed frame rate (vertically stacked PNG strips).
 * Pure Java so it is unit-tested.
 */
public final class FrameTimeline {
    private final int frames;
    /** Cumulative end time of each frame in ms, or null for fixed-rate timing. */
    private final long[] ends;

    private FrameTimeline(int frames, long[] ends) {
        this.frames = frames;
        this.ends = ends;
    }

    /** Fixed-rate timeline (frame duration comes from the caller's fps). */
    public static FrameTimeline fixed(int frames) {
        if (frames < 1) throw new IllegalArgumentException("frames must be >= 1");
        return new FrameTimeline(frames, null);
    }

    /** Per-frame durations in milliseconds; each must be positive. */
    public static FrameTimeline ofDelays(int[] delaysMs) {
        if (delaysMs == null || delaysMs.length == 0) throw new IllegalArgumentException("no frames");
        long[] ends = new long[delaysMs.length];
        long t = 0;
        for (int i = 0; i < delaysMs.length; i++) {
            if (delaysMs[i] <= 0) throw new IllegalArgumentException("frame " + i + " has no duration");
            t += delaysMs[i];
            ends[i] = t;
        }
        return new FrameTimeline(delaysMs.length, ends);
    }

    public int frameCount() {
        return frames;
    }

    public boolean hasOwnTiming() {
        return ends != null;
    }

    /** Total loop length in ms (0 for fixed-rate timelines). */
    public long loopMs() {
        return ends == null ? 0L : ends[ends.length - 1];
    }

    /**
     * @param nowMs any monotonic or wall-clock time in ms (negative values are allowed)
     * @param fps   frame rate used only for fixed-rate timelines (clamped to >= 1)
     */
    public int frameAt(long nowMs, int fps) {
        if (frames == 1) return 0;
        if (ends == null) {
            long frameMs = 1000L / Math.max(1, fps);
            return (int) Math.floorMod(Math.floorDiv(nowMs, Math.max(1L, frameMs)), (long) frames);
        }
        long t = Math.floorMod(nowMs, ends[ends.length - 1]);
        // First frame whose end is strictly after t.
        int lo = 0;
        int hi = ends.length - 1;
        while (lo < hi) {
            int mid = (lo + hi) >>> 1;
            if (ends[mid] > t) hi = mid;
            else lo = mid + 1;
        }
        return lo;
    }
}
