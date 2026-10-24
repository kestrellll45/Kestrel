package com.kestrel.input;

import com.kestrel.KestrelClient;
import net.minecraft.client.Minecraft;
import net.minecraft.client.settings.KeyBinding;
import net.minecraftforge.client.event.MouseEvent;
import net.minecraftforge.fml.client.registry.ClientRegistry;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;
import org.lwjgl.input.Keyboard;
import org.lwjgl.input.Mouse;

/**
 * Keybinds and click tracking. Uses only the game's own input state and
 * Forge's in-game MouseEvent — no global keyboard/mouse hooks.
 */
public final class InputManager {
    public static final String CATEGORY = "Kestrel";

    private final KestrelClient client;
    private final CpsTracker leftCps = new CpsTracker();
    private final CpsTracker rightCps = new CpsTracker();

    private final KeyBinding openMenu = new KeyBinding("Open Kestrel menu", Keyboard.KEY_RSHIFT, CATEGORY);
    private final KeyBinding openHudEditor = new KeyBinding("Open HUD editor", Keyboard.KEY_NONE, CATEGORY);

    public InputManager(KestrelClient client) {
        this.client = client;
    }

    public void registerKeybinds() {
        // Bindings are saved by Minecraft in options.txt like vanilla ones.
        ClientRegistry.registerKeyBinding(openMenu);
        ClientRegistry.registerKeyBinding(openHudEditor);
    }

    @SubscribeEvent
    public void onMouse(MouseEvent event) {
        if (!event.buttonstate) return;
        long now = System.currentTimeMillis();
        if (event.button == 0) {
            leftCps.click(now);
        } else if (event.button == 1) {
            rightCps.click(now);
        }
    }

    @SubscribeEvent
    public void onTick(TickEvent.ClientTickEvent event) {
        if (event.phase != TickEvent.Phase.END) return;
        Minecraft mc = Minecraft.getMinecraft();
        if (mc.currentScreen != null) {
            return;
        }
        while (openMenu.isPressed()) {
            client.openSettings();
        }
        while (openHudEditor.isPressed()) {
            client.openHudEditor();
        }
    }

    public int leftCps() {
        return leftCps.cps(System.currentTimeMillis());
    }

    public int rightCps() {
        return rightCps.cps(System.currentTimeMillis());
    }

    /** Physical state of a key code as Minecraft encodes it (negative = mouse). */
    public static boolean isPhysicallyDown(int keyCode) {
        if (keyCode == 0) return false;
        if (keyCode < 0) {
            int button = keyCode + 100;
            return button >= 0 && button < Mouse.getButtonCount() && Mouse.isButtonDown(button);
        }
        return keyCode < Keyboard.KEYBOARD_SIZE && Keyboard.isKeyDown(keyCode);
    }
}
