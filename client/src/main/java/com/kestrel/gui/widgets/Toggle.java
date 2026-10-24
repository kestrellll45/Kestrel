package com.kestrel.gui.widgets;

import com.kestrel.gui.Theme;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;

import java.util.function.Consumer;
import java.util.function.Supplier;

public final class Toggle extends Widget {
    private final String label;
    private final Supplier<Boolean> get;
    private final Consumer<Boolean> set;

    public Toggle(String label, Supplier<Boolean> get, Consumer<Boolean> set) {
        this.label = label;
        this.get = get;
        this.set = set;
    }

    @Override
    public void draw(Minecraft mc, int mx, int my) {
        boolean on = get.get();
        RenderUtil.softRect(x, y, x + w, y + h, isHovered(mx, my) ? Theme.CARD_HOVER : Theme.CARD);
        mc.fontRendererObj.drawString(fit(mc, label, w - 40), x + 6, y + (h - 8) / 2, Theme.TEXT, false);
        int sx = x + w - 28;
        int sy = y + (h - 10) / 2;
        RenderUtil.softRect(sx, sy, sx + 22, sy + 10, on ? Theme.ACCENT : Theme.TRACK);
        int kx = on ? sx + 13 : sx + 1;
        RenderUtil.softRect(kx, sy + 1, kx + 8, sy + 9, 0xFFFFFFFF);
    }

    @Override
    public boolean mouseClicked(int mx, int my, int button) {
        if (button != 0 || !isHovered(mx, my)) return false;
        set.accept(!get.get());
        return true;
    }
}
