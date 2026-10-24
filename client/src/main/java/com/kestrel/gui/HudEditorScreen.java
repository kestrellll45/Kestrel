package com.kestrel.gui;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import com.kestrel.gui.widgets.ActionButton;
import com.kestrel.hud.HudElement;
import com.kestrel.hud.HudManager;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.gui.GuiScreen;
import net.minecraft.client.gui.ScaledResolution;
import org.lwjgl.input.Mouse;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;

/**
 * Drag HUD modules to move them, scroll over one to scale it, right-click to
 * show/hide it. Positions snap to screen edges and centre lines.
 */
public final class HudEditorScreen extends GuiScreen {
    private static final int SNAP = 4;

    private final KestrelClient client;
    private final List<ActionButton> buttons = new ArrayList<ActionButton>();
    private final float[] pos = new float[2];
    private HudElement dragging;
    private float dragOffX;
    private float dragOffY;
    private boolean snapV;
    private boolean snapH;

    public HudEditorScreen(KestrelClient client) {
        this.client = client;
    }

    @Override
    public void initGui() {
        buttons.clear();
        int bw = Math.min(90, (width - 40) / 3);
        int y = height / 2 + 30;
        int x0 = width / 2 - (bw * 3 + 8) / 2;
        addButton(x0, y, bw, "Reset", false, () -> {
            for (HudElement e : client.hud().elements()) e.resetPosition();
            client.onConfigChanged();
        });
        addButton(x0 + bw + 4, y, bw, "Settings", false, client::openSettings);
        addButton(x0 + (bw + 4) * 2, y, bw, "Done", true, () -> mc.displayGuiScreen(null));
    }

    private void addButton(int x, int y, int w, String label, boolean primary, Runnable r) {
        ActionButton b = new ActionButton(label, primary, r);
        b.x = x;
        b.y = y;
        b.w = w;
        b.h = 18;
        buttons.add(b);
    }

    @Override
    public void drawScreen(int mouseX, int mouseY, float partialTicks) {
        RenderUtil.rect(0, 0, width, height, 0x50000000);
        // Centre guides (highlighted while snapping to them).
        RenderUtil.rect(width / 2, 0, width / 2 + 1, height, snapV ? 0x90E8833A : 0x20FFFFFF);
        RenderUtil.rect(0, height / 2, width, height / 2 + 1, snapH ? 0x90E8833A : 0x20FFFFFF);

        ScaledResolution res = new ScaledResolution(mc);
        client.hud().renderAll(mc, res, true);

        HudElement hovered = dragging != null ? dragging : elementAt(mouseX, mouseY, res);
        for (HudElement e : client.hud().elements()) {
            Box b = box(e, res);
            if (b == null) continue;
            if (!e.cfg().enabled) {
                RenderUtil.rect(b.x1, b.y1, b.x2, b.y2, 0x90000000);
                String off = "hidden";
                fontRendererObj.drawString(off, (b.x1 + b.x2 - fontRendererObj.getStringWidth(off)) / 2, (b.y1 + b.y2) / 2 - 4, Theme.MUTED);
            }
            RenderUtil.outline(b.x1 - 1, b.y1 - 1, b.x2 + 1, b.y2 + 1, e == hovered ? Theme.ACCENT : 0x60FFFFFF);
        }
        if (hovered != null) {
            Box b = box(hovered, res);
            if (b != null) {
                String label = hovered.name + "  " + Math.round(hovered.cfg().scale * 100) + "%";
                int ly = b.y1 > 12 ? b.y1 - 11 : b.y2 + 3;
                fontRendererObj.drawStringWithShadow(label, b.x1, ly, Theme.ACCENT);
            }
        }

        String help = "Drag to move  -  Scroll to scale  -  Right-click to show/hide";
        fontRendererObj.drawStringWithShadow(help, (width - fontRendererObj.getStringWidth(help)) / 2, height / 2 + 16, Theme.TEXT);
        for (ActionButton b : buttons) b.draw(mc, mouseX, mouseY);
        super.drawScreen(mouseX, mouseY, partialTicks);
    }

    private static final class Box {
        int x1, y1, x2, y2;
    }

    private Box box(HudElement e, ScaledResolution res) {
        int w = e.width(true);
        int h = e.height(true);
        if (w <= 0 || h <= 0) return null;
        HudManager.screenPos(e, res, w, h, pos);
        float s = e.cfg().scale;
        Box b = new Box();
        b.x1 = (int) pos[0];
        b.y1 = (int) pos[1];
        b.x2 = (int) (pos[0] + w * s);
        b.y2 = (int) (pos[1] + h * s);
        return b;
    }

    private HudElement elementAt(int mx, int my, ScaledResolution res) {
        List<HudElement> els = client.hud().elements();
        for (int i = els.size() - 1; i >= 0; i--) {
            Box b = box(els.get(i), res);
            if (b != null && mx >= b.x1 && mx < b.x2 && my >= b.y1 && my < b.y2) return els.get(i);
        }
        return null;
    }

    @Override
    protected void mouseClicked(int mouseX, int mouseY, int button) throws IOException {
        for (ActionButton b : buttons) {
            if (b.mouseClicked(mouseX, mouseY, button)) return;
        }
        ScaledResolution res = new ScaledResolution(mc);
        HudElement e = elementAt(mouseX, mouseY, res);
        if (e == null) return;
        if (button == 1) {
            e.cfg().enabled = !e.cfg().enabled;
            client.configManager().markDirty();
        } else if (button == 0) {
            Box b = box(e, res);
            dragging = e;
            dragOffX = mouseX - b.x1;
            dragOffY = mouseY - b.y1;
        }
    }

    @Override
    protected void mouseClickMove(int mouseX, int mouseY, int button, long time) {
        if (dragging == null) return;
        ScaledResolution res = new ScaledResolution(mc);
        ClientConfig.HudElementConfig c = dragging.cfg();
        float sw = res.getScaledWidth();
        float sh = res.getScaledHeight();
        float w = dragging.width(true) * c.scale;
        float h = dragging.height(true) * c.scale;
        float x = mouseX - dragOffX;
        float y = mouseY - dragOffY;
        snapV = false;
        snapH = false;
        // Snap to edges and centre lines.
        if (Math.abs(x) < SNAP) x = 0;
        if (Math.abs(x + w - sw) < SNAP) x = sw - w;
        if (Math.abs(x + w / 2 - sw / 2) < SNAP) {
            x = sw / 2 - w / 2;
            snapV = true;
        }
        if (Math.abs(y) < SNAP) y = 0;
        if (Math.abs(y + h - sh) < SNAP) y = sh - h;
        if (Math.abs(y + h / 2 - sh / 2) < SNAP) {
            y = sh / 2 - h / 2;
            snapH = true;
        }
        x = Math.max(0, Math.min(sw - w, x));
        y = Math.max(0, Math.min(sh - h, y));
        c.x = x / sw;
        c.y = y / sh;
    }

    @Override
    protected void mouseReleased(int mouseX, int mouseY, int state) {
        if (dragging != null) {
            dragging = null;
            snapV = false;
            snapH = false;
            client.configManager().markDirty();
        }
        super.mouseReleased(mouseX, mouseY, state);
    }

    @Override
    public void handleMouseInput() throws IOException {
        super.handleMouseInput();
        int wheel = Mouse.getEventDWheel();
        if (wheel == 0) return;
        int mx = Mouse.getEventX() * width / mc.displayWidth;
        int my = height - Mouse.getEventY() * height / mc.displayHeight - 1;
        HudElement e = elementAt(mx, my, new ScaledResolution(mc));
        if (e == null) return;
        ClientConfig.HudElementConfig c = e.cfg();
        c.scale = Math.max(0.5f, Math.min(3.0f, Math.round((c.scale + (wheel > 0 ? 0.1f : -0.1f)) * 10) / 10f));
        client.configManager().markDirty();
    }

    @Override
    public void onGuiClosed() {
        client.configManager().markDirty();
    }

    @Override
    public boolean doesGuiPauseGame() {
        return false;
    }
}
