package com.kestrel.gui.widgets;

import com.kestrel.gui.Theme;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;

import java.util.List;
import java.util.function.Consumer;
import java.util.function.Supplier;

/** Cycles through options: left click = next, right click = previous. */
public final class Choice extends Widget {
    private final String label;
    private final List<String> ids;
    private final List<String> names;
    private final Supplier<String> get;
    private final Consumer<String> set;

    public Choice(String label, List<String> ids, List<String> names, Supplier<String> get, Consumer<String> set) {
        this.label = label;
        this.ids = ids;
        this.names = names;
        this.get = get;
        this.set = set;
    }

    @Override
    public void draw(Minecraft mc, int mx, int my) {
        RenderUtil.softRect(x, y, x + w, y + h, isHovered(mx, my) ? Theme.CARD_HOVER : Theme.CARD);
        mc.fontRendererObj.drawString(fit(mc, label, w / 2 - 10), x + 6, y + (h - 8) / 2, Theme.TEXT, false);
        int i = ids.indexOf(get.get());
        String v = "< " + (i >= 0 ? names.get(i) : "?") + " >";
        v = fit(mc, v, w / 2 - 6);
        int vw = mc.fontRendererObj.getStringWidth(v);
        mc.fontRendererObj.drawString(v, x + w - vw - 6, y + (h - 8) / 2, Theme.ACCENT, false);
    }

    @Override
    public boolean mouseClicked(int mx, int my, int button) {
        if (!isHovered(mx, my) || ids.isEmpty() || (button != 0 && button != 1)) return false;
        int i = ids.indexOf(get.get());
        int n = ids.size();
        int next = button == 0 ? (i + 1) % n : (i - 1 + n) % n;
        set.accept(ids.get(Math.max(0, next)));
        return true;
    }
}
