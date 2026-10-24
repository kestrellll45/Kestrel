package com.kestrel.gui.widgets;

import net.minecraft.client.Minecraft;

/** Minimal retained-mode widget. Coordinates are scaled GUI units. */
public abstract class Widget {
    public int x;
    public int y;
    public int w;
    public int h = 20;

    public abstract void draw(Minecraft mc, int mouseX, int mouseY);

    /** @return true if the click was consumed */
    public boolean mouseClicked(int mouseX, int mouseY, int button) {
        return false;
    }

    public void mouseDragged(int mouseX, int mouseY) {
    }

    public void mouseReleased(int mouseX, int mouseY) {
    }

    public boolean isHovered(int mx, int my) {
        return mx >= x && mx < x + w && my >= y && my < y + h;
    }

    protected static String fit(Minecraft mc, String s, int width) {
        if (mc.fontRendererObj.getStringWidth(s) <= width) return s;
        return mc.fontRendererObj.trimStringToWidth(s, Math.max(0, width - 6)) + "...";
    }
}
