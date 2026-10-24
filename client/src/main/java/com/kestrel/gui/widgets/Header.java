package com.kestrel.gui.widgets;

import com.kestrel.gui.Theme;
import net.minecraft.client.Minecraft;

import java.util.List;

/** Section title or wrapped explanatory text. */
public final class Header extends Widget {
    private final String text;
    private final boolean title;
    private final int color;
    private List<String> lines;
    private int wrappedFor = -1;

    public Header(String text, boolean title) {
        this(text, title, title ? Theme.TEXT : Theme.MUTED);
    }

    public Header(String text, boolean title, int color) {
        this.text = text;
        this.title = title;
        this.color = color;
        this.h = 14;
    }

    /** Height depends on width for wrapped text. */
    public int layout(Minecraft mc, int width) {
        if (title) {
            h = 16;
        } else {
            if (wrappedFor != width) {
                lines = mc.fontRendererObj.listFormattedStringToWidth(text, width);
                wrappedFor = width;
            }
            h = lines.size() * 10 + 4;
        }
        return h;
    }

    @Override
    public void draw(Minecraft mc, int mouseX, int mouseY) {
        if (title) {
            mc.fontRendererObj.drawString(text, x, y + 5, color, false);
            return;
        }
        if (lines == null) layout(mc, w);
        for (int i = 0; i < lines.size(); i++) {
            mc.fontRendererObj.drawString(lines.get(i), x, y + 2 + i * 10, color, false);
        }
    }
}
