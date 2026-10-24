package com.kestrel.hud.elements;

import com.kestrel.KestrelClient;
import com.kestrel.hud.TextHudElement;
import net.minecraft.client.Minecraft;

import java.util.List;

public final class CpsElement extends TextHudElement {
    public CpsElement(KestrelClient client) {
        super(client, "cps", "CPS", true, 0.005f, 0.045f);
    }

    @Override
    protected void buildLines(Minecraft mc, List<String> out) {
        out.add(client.input().leftCps() + " | " + client.input().rightCps() + " CPS");
    }

    @Override
    protected void previewLines(List<String> out) {
        out.add("12 | 4 CPS");
    }
}
