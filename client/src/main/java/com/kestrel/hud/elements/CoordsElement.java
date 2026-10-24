package com.kestrel.hud.elements;

import com.kestrel.KestrelClient;
import com.kestrel.hud.TextHudElement;
import net.minecraft.client.Minecraft;
import net.minecraft.entity.Entity;
import net.minecraft.util.MathHelper;

import java.util.List;

public final class CoordsElement extends TextHudElement {
    private static final String[] FACING = {"South (+Z)", "West (-X)", "North (-Z)", "East (+X)"};

    public CoordsElement(KestrelClient client) {
        super(client, "coords", "Coordinates", false, 0.005f, 0.30f);
    }

    @Override
    protected void buildLines(Minecraft mc, List<String> out) {
        Entity e = mc.getRenderViewEntity();
        if (e == null) return;
        out.add("X: " + MathHelper.floor_double(e.posX));
        out.add("Y: " + MathHelper.floor_double(e.getEntityBoundingBox().minY));
        out.add("Z: " + MathHelper.floor_double(e.posZ));
        int f = MathHelper.floor_double(e.rotationYaw * 4.0F / 360.0F + 0.5D) & 3;
        out.add(FACING[f]);
    }

    @Override
    protected void previewLines(List<String> out) {
        out.add("X: 128");
        out.add("Y: 64");
        out.add("Z: -256");
        out.add("North (-Z)");
    }
}
