package com.kestrel.hud;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.FontRenderer;

import java.util.ArrayList;
import java.util.List;

/** HUD element made of one or more text lines with optional background. */
public abstract class TextHudElement extends HudElement {
    protected static final int PAD = 3;

    private final List<String> lines = new ArrayList<String>(4);
    private int cachedWidth;

    protected TextHudElement(KestrelClient client, String id, String name, boolean on, float x, float y) {
        super(client, id, name, on, x, y);
    }

    /** Fill {@code out} with the lines to show. */
    protected abstract void buildLines(Minecraft mc, List<String> out);

    /** Lines shown in the editor when there is no real content. */
    protected abstract void previewLines(List<String> out);

    @Override
    protected final void refresh(Minecraft mc) {
        lines.clear();
        if (hasContent(mc)) {
            buildLines(mc, lines);
        }
        cachedWidth = measure(mc.fontRendererObj, lines);
    }

    private static int measure(FontRenderer fr, List<String> ls) {
        int w = 0;
        for (int i = 0; i < ls.size(); i++) {
            w = Math.max(w, fr.getStringWidth(ls.get(i)));
        }
        return w;
    }

    private List<String> shown(boolean preview) {
        if (preview && lines.isEmpty()) {
            List<String> p = new ArrayList<String>(4);
            previewLines(p);
            return p;
        }
        return lines;
    }

    @Override
    public int width(boolean preview) {
        List<String> ls = shown(preview);
        int w = ls == lines ? cachedWidth : measure(Minecraft.getMinecraft().fontRendererObj, ls);
        return ls.isEmpty() ? 0 : w + PAD * 2;
    }

    @Override
    public int height(boolean preview) {
        List<String> ls = shown(preview);
        return ls.isEmpty() ? 0 : ls.size() * 10 + PAD * 2 - 1;
    }

    @Override
    protected void render(Minecraft mc, boolean preview) {
        List<String> ls = shown(preview);
        if (ls.isEmpty()) return;
        ClientConfig.HudElementConfig c = cfg();
        int w = width(preview);
        int h = height(preview);
        if (c.background) {
            RenderUtil.softRect(0, 0, w, h, 0x80000000);
        }
        FontRenderer fr = mc.fontRendererObj;
        for (int i = 0; i < ls.size(); i++) {
            fr.drawString(ls.get(i), PAD, PAD + i * 10, c.textColor, c.textShadow);
        }
    }
}
