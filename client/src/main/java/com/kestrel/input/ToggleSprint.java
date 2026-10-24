package com.kestrel.input;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import net.minecraft.client.Minecraft;
import net.minecraft.client.entity.EntityPlayerSP;
import net.minecraft.client.settings.KeyBinding;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;

/**
 * Toggle sprint: pressing the vanilla sprint key flips a toggle; while
 * toggled we hold the sprint key binding down. Vanilla sprint rules still
 * decide whether sprinting actually happens (hunger, blindness, using an
 * item, sneaking, not moving forward, collisions), so behaviour stays
 * predictable and server-legal.
 */
public final class ToggleSprint {
    private final KestrelClient client;
    private boolean lastPhysical;
    /** True while we are the reason the sprint binding is held. */
    private boolean forcing;

    public ToggleSprint(KestrelClient client) {
        this.client = client;
    }

    @SubscribeEvent
    public void onTick(TickEvent.ClientTickEvent event) {
        Minecraft mc = Minecraft.getMinecraft();
        ClientConfig.Sprint cfg = client.config().sprint;
        KeyBinding sprint = mc.gameSettings.keyBindSprint;
        int code = sprint.getKeyCode();
        boolean physical = InputManager.isPhysicallyDown(code);

        if (!cfg.toggleSprint) {
            release(code, physical);
            lastPhysical = physical;
            return;
        }

        // Toggle on the press edge, only while playing (not typing in chat/GUIs).
        if (event.phase == TickEvent.Phase.START && mc.currentScreen == null && physical && !lastPhysical) {
            cfg.toggled = !cfg.toggled;
            client.configManager().markDirty();
        }
        if (event.phase == TickEvent.Phase.START) {
            lastPhysical = physical;
        }

        EntityPlayerSP player = mc.thePlayer;
        if (cfg.toggled && player != null && mc.currentScreen == null) {
            KeyBinding.setKeyBindState(code, true);
            forcing = true;
        } else {
            release(code, physical);
        }
    }

    private void release(int code, boolean physical) {
        if (forcing) {
            KeyBinding.setKeyBindState(code, physical);
            forcing = false;
        }
    }

    public boolean isToggled() {
        return client.config().sprint.toggleSprint && client.config().sprint.toggled;
    }
}
