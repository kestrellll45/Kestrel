package com.kestrel.gui.widgets;

import com.kestrel.gui.Theme;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;

import java.util.function.Consumer;
import java.util.function.Function;
import java.util.function.Supplier;

public final class Slider extends Widget {
    private final String label;
    private final float min;
    private final float max;
    private final float step;
    private final Supplier<Float> get;
    private final Consumer<Float> set;
    private final Function<Float, String> format;
    private boolean dragging;

    public Slider(String label, float min, float max, float step, Supplier<Float> get, Consumer<Float> set, Function<Float, String> format) {
        this.label = label;
        this.min = min;
        this.max = max;
        this.step = step;
        this.get = get;
        this.set = set;
        this.format = format;
    }

    private int trackX() {
        return x + w / 2;
    }

    private int trackW() {
        return w / 2 - 8;
    }

    @Override
    public void draw(Minecraft mc, int mx, int my) {
        RenderUtil.softRect(x, y, x + w, y + h, isHovered(mx, my) || dragging ? Theme.CARD_HOVER : Theme.CARD);
        String value = format.apply(get.get());
        mc.fontRendererObj.drawString(fit(mc, label, w / 2 - 10), x + 6, y + (h - 8) / 2, Theme.TEXT, false);
        int tx = trackX();
        int tw = trackW();
        int ty = y + h / 2;
        float f = (get.get() - min) / (max - min);
        f = Math.max(0, Math.min(1, f));
        RenderUtil.rect(tx, ty - 1, tx + tw, ty + 1, Theme.TRACK);
        RenderUtil.rect(tx, ty - 1, tx + (int) (tw * f), ty + 1, Theme.ACCENT);
        int kx = tx + (int) (tw * f);
        RenderUtil.softRect(kx - 3, ty - 5, kx + 3, ty + 5, 0xFFFFFFFF);
        int vw = mc.fontRendererObj.getStringWidth(value);
        mc.fontRendererObj.drawString(value, tx - vw - 6, y + (h - 8) / 2, Theme.MUTED, false);
    }

    @Override
    public boolean mouseClicked(int mx, int my, int button) {
        if (button != 0 || !isHovered(mx, my) || mx < trackX() - 4) return false;
        dragging = true;
        update(mx);
        return true;
    }

    @Override
    public void mouseDragged(int mx, int my) {
        if (dragging) update(mx);
    }

    @Override
    public void mouseReleased(int mx, int my) {
        dragging = false;
    }

    private void update(int mx) {
        float f = (mx - trackX()) / (float) trackW();
        f = Math.max(0, Math.min(1, f));
        float v = min + f * (max - min);
        if (step > 0) v = Math.round(v / step) * step;
        v = Math.max(min, Math.min(max, v));
        if (Math.abs(v - get.get()) > 1e-6f) set.accept(v);
    }
}
