package com.kestrel.gui.widgets;

import com.kestrel.gui.Theme;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;

/** A framed area with custom drawing (crosshair/cape previews). */
public final class Preview extends Widget {
    public interface Painter {
        /** Draw centred at (cx, cy) within the preview box. */
        void paint(Minecraft mc, int cx, int cy, int boxW, int boxH);
    }

    private final Painter painter;

    public Preview(int height, Painter painter) {
        this.h = height;
        this.painter = painter;
    }

    @Override
    public void draw(Minecraft mc, int mx, int my) {
        RenderUtil.softRect(x, y, x + w, y + h, 0xFF101114);
        RenderUtil.outline(x, y, x + w, y + h, Theme.TRACK);
        painter.paint(mc, x + w / 2, y + h / 2, w, h);
    }
}
