package com.kestrel.hud.elements;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import com.kestrel.hud.HudElement;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.FontRenderer;
import net.minecraft.client.settings.GameSettings;
import net.minecraft.client.settings.KeyBinding;

/**
 * WASD + mouse buttons (with CPS) + space bar, read from the game's own key
 * bindings so rebinding keys is reflected automatically.
 */
public final class KeystrokesElement extends HudElement {
    private static final int KEY = 22;
    private static final int GAP = 2;
    private static final int WIDTH = KEY * 3 + GAP * 2;
    private static final int HEIGHT = KEY * 2 + GAP + (KEY - 2) + GAP + 12 + GAP;
    private static final int IDLE = 0x80000000;
    private static final int PRESSED = 0xC0FFFFFF;

    public KeystrokesElement(KestrelClient client) {
        super(client, "keystrokes", "Keystrokes", true, 0.005f, 0.12f);
    }

    @Override
    protected void refresh(Minecraft mc) {
        // Reads live input every frame in render(); nothing to cache.
    }

    @Override
    public boolean realtime() {
        return true;
    }

    @Override
    public int width(boolean preview) {
        return WIDTH;
    }

    @Override
    public int height(boolean preview) {
        return HEIGHT;
    }

    @Override
    protected void render(Minecraft mc, boolean preview) {
        GameSettings gs = mc.gameSettings;
        ClientConfig.HudElementConfig c = cfg();
        FontRenderer fr = mc.fontRendererObj;
        boolean live = !preview;
        key(fr, c, KEY + GAP, 0, KEY, KEY, label(gs.keyBindForward), live && gs.keyBindForward.isKeyDown());
        key(fr, c, 0, KEY + GAP, KEY, KEY, label(gs.keyBindLeft), live && gs.keyBindLeft.isKeyDown());
        key(fr, c, KEY + GAP, KEY + GAP, KEY, KEY, label(gs.keyBindBack), live && gs.keyBindBack.isKeyDown());
        key(fr, c, (KEY + GAP) * 2, KEY + GAP, KEY, KEY, label(gs.keyBindRight), live && gs.keyBindRight.isKeyDown());

        int y = (KEY + GAP) * 2;
        int half = (WIDTH - GAP) / 2;
        int mh = KEY - 2;
        String lmb = preview ? "12 CPS" : client.input().leftCps() + " CPS";
        String rmb = preview ? "4 CPS" : client.input().rightCps() + " CPS";
        key(fr, c, 0, y, half, mh, lmb, live && gs.keyBindAttack.isKeyDown());
        key(fr, c, half + GAP, y, WIDTH - half - GAP, mh, rmb, live && gs.keyBindUseItem.isKeyDown());

        y += mh + GAP;
        key(fr, c, 0, y, WIDTH, 12, "-----", live && gs.keyBindJump.isKeyDown());
    }

    private static void key(FontRenderer fr, ClientConfig.HudElementConfig c, int x, int y, int w, int h, String text, boolean down) {
        if (c.background || down) {
            RenderUtil.softRect(x, y, x + w, y + h, down ? PRESSED : IDLE);
        }
        int color = down ? 0xFF000000 : c.textColor;
        int tw = fr.getStringWidth(text);
        fr.drawString(text, x + (w - tw) / 2f, y + (h - 8) / 2f + 1, color, c.textShadow && !down);
    }

    private static String label(KeyBinding kb) {
        String name = GameSettings.getKeyDisplayString(kb.getKeyCode());
        return name.length() > 3 ? name.substring(0, 3) : name;
    }
}
