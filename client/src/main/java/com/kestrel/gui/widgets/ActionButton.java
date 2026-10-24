package com.kestrel.gui.widgets;

import com.kestrel.gui.Theme;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;
import net.minecraft.client.audio.PositionedSoundRecord;
import net.minecraft.util.ResourceLocation;

public final class ActionButton extends Widget {
    private final String label;
    private final Runnable action;
    private final boolean primary;
    public boolean enabled = true;

    public ActionButton(String label, boolean primary, Runnable action) {
        this.label = label;
        this.primary = primary;
        this.action = action;
    }

    @Override
    public void draw(Minecraft mc, int mx, int my) {
        boolean hov = enabled && isHovered(mx, my);
        int bg = !enabled ? Theme.TRACK : primary ? (hov ? 0xFFF09A5A : Theme.ACCENT) : (hov ? Theme.CARD_HOVER : Theme.CARD);
        RenderUtil.softRect(x, y, x + w, y + h, bg);
        if (!primary && hov) RenderUtil.outline(x, y, x + w, y + h, Theme.ACCENT_DIM);
        String s = fit(mc, label, w - 8);
        int sw = mc.fontRendererObj.getStringWidth(s);
        int col = !enabled ? Theme.MUTED : primary ? 0xFF111111 : Theme.TEXT;
        mc.fontRendererObj.drawString(s, x + (w - sw) / 2, y + (h - 8) / 2, col, false);
    }

    @Override
    public boolean mouseClicked(int mx, int my, int button) {
        if (button != 0 || !enabled || !isHovered(mx, my)) return false;
        Minecraft.getMinecraft().getSoundHandler().playSound(PositionedSoundRecord.create(new ResourceLocation("gui.button.press"), 1.0F));
        action.run();
        return true;
    }
}
