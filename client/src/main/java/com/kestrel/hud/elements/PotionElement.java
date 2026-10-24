package com.kestrel.hud.elements;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import com.kestrel.hud.HudElement;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.renderer.GlStateManager;
import net.minecraft.client.resources.I18n;
import net.minecraft.potion.Potion;
import net.minecraft.potion.PotionEffect;
import net.minecraft.util.ResourceLocation;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;

/** Active potion effects with icon, level and remaining time. */
public final class PotionElement extends HudElement {
    private static final ResourceLocation INVENTORY = new ResourceLocation("textures/gui/container/inventory.png");
    private static final String[] ROMAN = {"", " II", " III", " IV", " V", " VI", " VII", " VIII", " IX", " X"};
    private static final int ROW = 20;

    private final List<Row> rows = new ArrayList<Row>();
    private int textWidth;

    private static final class Row {
        int icon = -1;
        String name;
        String time;
    }

    public PotionElement(KestrelClient client) {
        super(client, "potions", "Potion effects", false, 0.88f, 0.30f);
    }

    @Override
    protected void refresh(Minecraft mc) {
        rows.clear();
        textWidth = 0;
        if (mc.thePlayer == null) return;
        Collection<PotionEffect> effects = mc.thePlayer.getActivePotionEffects();
        for (PotionEffect e : effects) {
            int id = e.getPotionID();
            if (id < 0 || id >= Potion.potionTypes.length) continue;
            Potion p = Potion.potionTypes[id];
            if (p == null) continue;
            Row r = new Row();
            r.icon = p.hasStatusIcon() ? p.getStatusIconIndex() : -1;
            int amp = e.getAmplifier();
            r.name = I18n.format(p.getName()) + (amp >= 0 && amp < ROMAN.length ? ROMAN[amp] : " " + (amp + 1));
            r.time = e.getIsPotionDurationMax() ? "**:**" : Potion.getDurationString(e);
            textWidth = Math.max(textWidth, Math.max(mc.fontRendererObj.getStringWidth(r.name), mc.fontRendererObj.getStringWidth(r.time)));
            rows.add(r);
        }
    }

    @Override
    public int width(boolean preview) {
        if (preview && rows.isEmpty()) return 22 + 60;
        return rows.isEmpty() ? 0 : 22 + textWidth + 3;
    }

    @Override
    public int height(boolean preview) {
        if (preview && rows.isEmpty()) return ROW * 2;
        return rows.size() * ROW;
    }

    @Override
    protected void render(Minecraft mc, boolean preview) {
        ClientConfig.HudElementConfig c = cfg();
        int w = width(preview);
        int h = height(preview);
        if (c.background) RenderUtil.softRect(0, 0, w, h, 0x80000000);
        if (preview && rows.isEmpty()) {
            line(mc, c, 0, 1, "Speed II", "1:30");
            line(mc, c, 1, 13, "Fire Resistance", "3:00");
            return;
        }
        for (int i = 0; i < rows.size(); i++) {
            Row r = rows.get(i);
            line(mc, c, i, r.icon, r.name, r.time);
        }
    }

    private static void line(Minecraft mc, ClientConfig.HudElementConfig c, int i, int icon, String name, String time) {
        int y = i * ROW;
        if (icon >= 0) {
            GlStateManager.color(1, 1, 1, 1);
            GlStateManager.enableBlend();
            mc.getTextureManager().bindTexture(INVENTORY);
            Gui.drawModalRectWithCustomSizedTexture(2, y + 1, (icon % 8) * 18, 198 + (icon / 8) * 18, 18, 18, 256, 256);
            GlStateManager.disableBlend();
        }
        mc.fontRendererObj.drawString(name, 22, y + 2, c.textColor, c.textShadow);
        mc.fontRendererObj.drawString(time, 22, y + 11, 0xFFAAAAAA, c.textShadow);
    }
}
