package com.kestrel.hud.elements;

import com.kestrel.KestrelClient;
import com.kestrel.hud.TextHudElement;
import net.minecraft.client.Minecraft;

import java.util.List;

public final class FpsElement extends TextHudElement {
    private int lastFps = -1;
    private String text = "";

    public FpsElement(KestrelClient client) {
        super(client, "fps", "FPS", true, 0.005f, 0.01f);
    }

    @Override
    protected boolean hasContent(Minecraft mc) {
        return true;
    }

    @Override
    protected void buildLines(Minecraft mc, List<String> out) {
        int fps = Minecraft.getDebugFPS();
        if (fps != lastFps) { // only re-format when the value changes
            lastFps = fps;
            text = fps + " FPS";
        }
        out.add(text);
    }

    @Override
    protected void previewLines(List<String> out) {
        out.add("240 FPS");
    }
}
