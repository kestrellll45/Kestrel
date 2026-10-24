package com.kestrel.gui.widgets;

import com.kestrel.gui.Theme;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;

import java.util.function.Consumer;
import java.util.function.Supplier;

/** Picks from {@link Theme#PALETTE}; left = next, right = previous. */
public final class ColorChoice extends Widget {
    private final String label;
    private final Supplier<Integer> get;
    private final Consumer<Integer> set;

    public ColorChoice(String label, Supplier<Integer> get, Consumer<Integer> set) {
        this.label = label;
        this.get = get;
        this.set = set;
    }

    @Override
    public void draw(Minecraft mc, int mx, int my) {
        RenderUtil.softRect(x, y, x + w, y + h, isHovered(mx, my) ? Theme.CARD_HOVER : Theme.CARD);
        mc.fontRendererObj.drawString(fit(mc, label, w - 40), x + 6, y + (h - 8) / 2, Theme.TEXT, false);
        int sx = x + w - 26;
        int sy = y + 4;
        RenderUtil.rect(sx - 1, sy - 1, sx + 21, sy + h - 7, 0xFF000000);
        RenderUtil.rect(sx, sy, sx + 20, sy + h - 8, get.get() | 0xFF000000);
    }

    @Override
    public boolean mouseClicked(int mx, int my, int button) {
        if (!isHovered(mx, my) || (button != 0 && button != 1)) return false;
        int cur = get.get() | 0xFF000000;
        int idx = -1;
        for (int i = 0; i < Theme.PALETTE.length; i++) if (Theme.PALETTE[i] == cur) idx = i;
        int n = Theme.PALETTE.length;
        int next = button == 0 ? (idx + 1) % n : (idx - 1 + n) % n;
        set.accept(Theme.PALETTE[next]);
        return true;
    }
}
