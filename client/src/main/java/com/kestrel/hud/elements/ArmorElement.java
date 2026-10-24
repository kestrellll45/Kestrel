package com.kestrel.hud.elements;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import com.kestrel.hud.HudElement;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.GlStateManager;
import net.minecraft.client.renderer.RenderHelper;
import net.minecraft.init.Items;
import net.minecraft.item.ItemStack;

/** Worn armour (helmet to boots) plus held item, with remaining durability. */
public final class ArmorElement extends HudElement {
    private static final int ROW = 17;
    private static final int TEXT_X = 19;

    private final ItemStack[] stacks = new ItemStack[5];
    private final String[] labels = new String[5];
    private int rows;
    private int textWidth;
    private ItemStack[] previewStacks;

    public ArmorElement(KestrelClient client) {
        super(client, "armor", "Armor status", false, 0.90f, 0.70f);
    }

    @Override
    protected void refresh(Minecraft mc) {
        rows = 0;
        textWidth = 0;
        if (mc.thePlayer == null) return;
        // armorInventory: 0 boots .. 3 helmet; show helmet first.
        for (int slot = 3; slot >= 0; slot--) {
            put(mc, mc.thePlayer.inventory.armorInventory[slot]);
        }
        put(mc, mc.thePlayer.getHeldItem());
    }

    private void put(Minecraft mc, ItemStack s) {
        if (s == null) return;
        stacks[rows] = s;
        String label;
        if (s.isItemStackDamageable()) {
            label = String.valueOf(s.getMaxDamage() - s.getItemDamage());
        } else if (s.stackSize > 1) {
            label = String.valueOf(s.stackSize);
        } else {
            label = "";
        }
        labels[rows] = label;
        textWidth = Math.max(textWidth, mc.fontRendererObj.getStringWidth(label));
        rows++;
    }

    @Override
    public int width(boolean preview) {
        if (preview && rows == 0) return TEXT_X + 18;
        return rows == 0 ? 0 : TEXT_X + textWidth + 2;
    }

    @Override
    public int height(boolean preview) {
        if (preview && rows == 0) return ROW * 4;
        return rows * ROW;
    }

    @Override
    protected void render(Minecraft mc, boolean preview) {
        ClientConfig.HudElementConfig c = cfg();
        if (preview && rows == 0) {
            if (previewStacks == null) {
                previewStacks = new ItemStack[]{new ItemStack(Items.diamond_helmet), new ItemStack(Items.diamond_chestplate),
                        new ItemStack(Items.diamond_leggings), new ItemStack(Items.diamond_boots)};
            }
            if (c.background) RenderUtil.softRect(0, 0, width(true), height(true), 0x80000000);
            for (int i = 0; i < previewStacks.length; i++) {
                drawItem(mc, previewStacks[i], i * ROW);
                mc.fontRendererObj.drawString("363", TEXT_X, i * ROW + 5, c.textColor, c.textShadow);
            }
            return;
        }
        if (c.background) RenderUtil.softRect(0, 0, width(false), height(false), 0x80000000);
        for (int i = 0; i < rows; i++) {
            drawItem(mc, stacks[i], i * ROW);
            mc.fontRendererObj.drawString(labels[i], TEXT_X, i * ROW + 5, durabilityColor(stacks[i], c.textColor), c.textShadow);
        }
    }

    private static void drawItem(Minecraft mc, ItemStack s, int y) {
        GlStateManager.pushMatrix();
        RenderHelper.enableGUIStandardItemLighting();
        mc.getRenderItem().renderItemAndEffectIntoGUI(s, 1, y);
        RenderHelper.disableStandardItemLighting();
        GlStateManager.popMatrix();
    }

    private static int durabilityColor(ItemStack s, int fallback) {
        if (s == null || !s.isItemStackDamageable() || s.getMaxDamage() <= 0) return fallback;
        float f = 1f - (float) s.getItemDamage() / s.getMaxDamage();
        if (f > 0.5f) return 0xFF55FF55;
        if (f > 0.2f) return 0xFFFFFF55;
        return 0xFFFF5555;
    }
}
