package com.kestrel.hud.elements;

import com.kestrel.KestrelClient;
import com.kestrel.hud.TextHudElement;
import com.kestrel.performance.FrameStats;
import net.minecraft.client.Minecraft;

import java.util.List;

/** Average FPS and 1% low over the last ~1000 frames: shows stutter that a plain FPS counter hides. */
public final class FrameStatsElement extends TextHudElement {
    public FrameStatsElement(KestrelClient client) {
        super(client, "framestats", "Frame stats", false, 0.005f, 0.08f);
    }

    @Override
    protected boolean hasContent(Minecraft mc) {
        return true;
    }

    @Override
    protected void buildLines(Minecraft mc, List<String> out) {
        FrameStats s = client.performance().frameStats();
        if (s.samples() < 30) {
            out.add("Frame stats: measuring...");
            return;
        }
        out.add("Avg " + s.avgFps() + " FPS");
        out.add("1% low " + s.onePercentLowFps() + " FPS");
        out.add(String.format("Max %.1f ms", s.maxFrameMs()));
    }

    @Override
    protected void previewLines(List<String> out) {
        out.add("Avg 240 FPS");
        out.add("1% low 180 FPS");
        out.add("Max 9.8 ms");
    }
}
