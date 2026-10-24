package com.kestrel.hud.elements;

import com.kestrel.KestrelClient;
import com.kestrel.hud.TextHudElement;
import net.minecraft.client.Minecraft;

import java.util.List;

/** Shows the state of client features that change input behaviour. */
public final class StatusElement extends TextHudElement {
    public StatusElement(KestrelClient client) {
        super(client, "status", "Status", true, 0.005f, 0.96f);
    }

    @Override
    protected void buildLines(Minecraft mc, List<String> out) {
        if (mc.thePlayer.isSneaking()) {
            out.add("[Sneaking]");
        } else if (client.toggleSprint().isToggled()) {
            out.add(mc.thePlayer.isSprinting() ? "[Sprinting (Toggled)]" : "[Sprint Toggled]");
        } else if (mc.thePlayer.isSprinting()) {
            out.add("[Sprinting]");
        }
    }

    @Override
    protected void previewLines(List<String> out) {
        out.add("[Sprinting (Toggled)]");
    }
}
