package com.kestrel.gui;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import com.kestrel.config.Preset;
import com.kestrel.cosmetics.CapeManager;
import com.kestrel.effects.KillEffectType;
import com.kestrel.gui.widgets.ActionButton;
import com.kestrel.gui.widgets.Choice;
import com.kestrel.gui.widgets.ColorChoice;
import com.kestrel.gui.widgets.Header;
import com.kestrel.gui.widgets.Preview;
import com.kestrel.gui.widgets.Slider;
import com.kestrel.gui.widgets.Toggle;
import com.kestrel.gui.widgets.Widget;
import com.kestrel.hud.CrosshairRenderer;
import com.kestrel.hud.HudElement;
import com.kestrel.performance.FrameStats;
import com.kestrel.rendering.SkyManager;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.GuiScreen;
import net.minecraft.client.renderer.GlStateManager;
import net.minecraft.util.ResourceLocation;
import org.lwjgl.input.Mouse;
import org.lwjgl.opengl.GL11;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;

/**
 * In-game settings (Right Shift by default). Every control is bound to a
 * real config value; changes apply immediately and are saved automatically.
 */
public final class ClientSettingsScreen extends GuiScreen {
    private enum Category {
        GENERAL("General"), HUD("HUD"), CROSSHAIR("Crosshair"), COSMETICS("Cosmetics"),
        SKY("Sky"), EFFECTS("Kill effects"), PERFORMANCE("Performance");

        final String label;

        Category(String label) {
            this.label = label;
        }
    }

    private static Category lastCategory = Category.GENERAL;

    private final KestrelClient client;
    private final List<Widget> content = new ArrayList<Widget>();
    private final List<ActionButton> tabs = new ArrayList<ActionButton>();
    private Category category = lastCategory;
    private int px, py, pw, ph, sideW, contentX, contentY, contentW, contentH;
    private int scroll;
    private int contentHeight;
    private Widget dragging;

    public ClientSettingsScreen(KestrelClient client) {
        this.client = client;
    }

    @Override
    public void initGui() {
        client.capes().reload(); // pick up newly added custom files
        client.sky().reload();
        pw = Math.min(460, width - 16);
        ph = Math.min(320, height - 16);
        px = (width - pw) / 2;
        py = (height - ph) / 2;
        sideW = pw < 360 ? 74 : 96;
        contentX = px + sideW + 10;
        contentY = py + 30;
        contentW = pw - sideW - 20;
        contentH = ph - 40;
        tabs.clear();
        int ty = py + 30;
        for (final Category c : Category.values()) {
            ActionButton b = new ActionButton(c.label, false, () -> select(c));
            b.x = px + 6;
            b.y = ty;
            b.w = sideW - 12;
            b.h = 18;
            tabs.add(b);
            ty += 20;
        }
        build();
    }

    private void select(Category c) {
        category = c;
        lastCategory = c;
        scroll = 0;
        build();
    }

    private ClientConfig cfg() {
        return client.config();
    }

    private void changed() {
        client.onConfigChanged();
    }

    private void build() {
        content.clear();
        ClientConfig c = cfg();
        switch (category) {
            case GENERAL:
                header("Kestrel " + client.version());
                text("Right Shift opens this menu (rebind under Controls > Kestrel). Settings save automatically.");
                content.add(new ActionButton("Edit HUD layout", true, client::openHudEditor));
                header("Movement");
                content.add(new Toggle("Toggle sprint", () -> c.sprint.toggleSprint, v -> {
                    c.sprint.toggleSprint = v;
                    changed();
                }));
                text("Press your sprint key once to keep sprinting. Vanilla rules still apply (hunger, blindness, using items).");
                header("Zoom & free look");
                text("Hold C to zoom, hold Left Alt to look around without turning. Rebind both under Controls > Kestrel.");
                content.add(new Slider("Zoom strength", 1.5f, 10f, 0.5f, () -> c.zoom.factor, v -> {
                    c.zoom.factor = v;
                    changed();
                }, v -> String.format("%.1fx", v)));
                content.add(new Toggle("Smooth zoom", () -> c.zoom.smooth, v -> {
                    c.zoom.smooth = v;
                    changed();
                }));
                content.add(new Slider("Zoom speed", 2f, 40f, 1f, () -> c.zoom.speed, v -> {
                    c.zoom.speed = v;
                    changed();
                }, v -> String.valueOf(Math.round(v))));
                content.add(new Toggle("Lower sensitivity while zoomed", () -> c.zoom.scaleSensitivity, v -> {
                    c.zoom.scaleSensitivity = v;
                    changed();
                }));
                content.add(new Toggle("Free look in third person", () -> c.freeLook.thirdPerson, v -> {
                    c.freeLook.thirdPerson = v;
                    changed();
                }));
                content.add(new Toggle("Smooth free-look return", () -> c.freeLook.smoothReturn, v -> {
                    c.freeLook.smoothReturn = v;
                    changed();
                }));
                text("Both are camera-only: your FOV setting, movement and what the server sees are unchanged.");
                header("Presets");
                for (final Preset p : Preset.values()) {
                    String label = p.label + (p.id.equals(c.activePreset) ? "  (active)" : "");
                    content.add(new ActionButton(label, false, () -> {
                        client.applyPreset(p);
                        build();
                    }));
                }
                text("Presets change effect/sky/animation settings only. Your HUD layout, crosshair, cape and keybinds are kept.");
                break;
            case HUD:
                content.add(new ActionButton("Open HUD editor (drag, scroll to scale)", true, client::openHudEditor));
                header("Modules");
                for (final HudElement e : client.hud().elements()) {
                    final ClientConfig.HudElementConfig ec = e.cfg();
                    content.add(new Toggle(e.name, () -> ec.enabled, v -> {
                        ec.enabled = v;
                        changed();
                    }));
                }
                header("Appearance (all modules)");
                content.add(new Toggle("Backgrounds", () -> allHud(true), v -> {
                    for (HudElement e : client.hud().elements()) e.cfg().background = v;
                    changed();
                }));
                content.add(new Toggle("Text shadow", () -> allHud(false), v -> {
                    for (HudElement e : client.hud().elements()) e.cfg().textShadow = v;
                    changed();
                }));
                content.add(new ColorChoice("Text colour", () -> client.hud().elements().get(0).cfg().textColor, v -> {
                    for (HudElement e : client.hud().elements()) e.cfg().textColor = v;
                    changed();
                }));
                content.add(new ActionButton("Reset all positions", false, () -> {
                    for (HudElement e : client.hud().elements()) e.resetPosition();
                    changed();
                }));
                break;
            case CROSSHAIR:
                final ClientConfig.Crosshair ch = c.crosshair;
                content.add(new Preview(48, (mc, cx, cy, bw, bh) -> {
                    if (!ch.custom) {
                        String s = "Vanilla crosshair";
                        mc.fontRendererObj.drawString(s, cx - mc.fontRendererObj.getStringWidth(s) / 2, cy - 4, Theme.MUTED);
                        return;
                    }
                    float sf = new net.minecraft.client.gui.ScaledResolution(mc).getScaleFactor();
                    GlStateManager.pushMatrix();
                    GlStateManager.translate(cx, cy, 0);
                    GlStateManager.scale(1f / sf, 1f / sf, 1f);
                    CrosshairRenderer.drawAtOrigin(ch);
                    GlStateManager.popMatrix();
                }));
                content.add(new Toggle("Custom crosshair", () -> ch.custom, v -> {
                    ch.custom = v;
                    changed();
                }));
                content.add(new Choice("Style", list("0", "1", "2", "3"), list("Cross", "Dot", "Cross + dot", "T"),
                        () -> String.valueOf(ch.style), v -> {
                    ch.style = Integer.parseInt(v);
                    changed();
                }));
                content.add(intSlider("Length", 1, 20, () -> ch.length, v -> ch.length = v));
                content.add(intSlider("Thickness", 1, 8, () -> ch.thickness, v -> ch.thickness = v));
                content.add(intSlider("Gap", 0, 15, () -> ch.gap, v -> ch.gap = v));
                content.add(new ColorChoice("Colour", () -> ch.color, v -> {
                    ch.color = v;
                    changed();
                }));
                content.add(new Toggle("Outline", () -> ch.outline, v -> {
                    ch.outline = v;
                    changed();
                }));
                content.add(new ColorChoice("Outline colour", () -> ch.outlineColor, v -> {
                    ch.outlineColor = v;
                    changed();
                }));
                content.add(new Toggle("Hide in third person", () -> ch.hideInThirdPerson, v -> {
                    ch.hideInThirdPerson = v;
                    changed();
                }));
                break;
            case COSMETICS:
                final ClientConfig.Cosmetics co = c.cosmetics;
                content.add(new Preview(64, (mc, cx, cy, bw, bh) -> drawCapePreview(mc, cx, cy)));
                content.add(new Toggle("Show cape", () -> co.capesEnabled, v -> {
                    co.capesEnabled = v;
                    changed();
                }));
                List<String> ids = new ArrayList<String>();
                List<String> names = new ArrayList<String>();
                for (CapeManager.Option o : client.capes().options()) {
                    ids.add(o.id);
                    names.add(o.name);
                }
                content.add(new Choice("Cape", ids, names, () -> co.cape, v -> {
                    co.cape = v;
                    changed();
                }));
                content.add(new Toggle("Animate capes", () -> co.animatedCapes, v -> {
                    co.animatedCapes = v;
                    changed();
                }));
                content.add(intSlider("Animation FPS (PNG strips)", 1, 30, () -> co.animationFps, v -> co.animationFps = v));
                content.add(new Toggle("Show players' assigned capes", () -> co.showAssignedCapes, v -> {
                    co.showAssignedCapes = v;
                    changed();
                }));
                content.add(new ActionButton("Open custom capes folder", false, () -> openFolder(client.capes().customDir())));
                text("Custom capes: PNG, 64x32 (or HD multiples like 128x64), max 2 MB; animated PNGs stack frames vertically. "
                        + "GIFs (max 4 MB) play with their own timing: a 2:1 GIF is used as a cape sheet, any other GIF is fitted onto both sides. "
                        + "Your cape is drawn by your client only. Not shown if you own an official cape.");
                if (client.capes().lastError() != null) {
                    content.add(new Header("Last error: " + client.capes().lastError(), false, Theme.ERR));
                }
                break;
            case SKY:
                final ClientConfig.Sky sk = c.sky;
                content.add(new Toggle("Custom sky", () -> sk.enabled, v -> {
                    sk.enabled = v;
                    changed();
                }));
                List<String> sids = new ArrayList<String>();
                List<String> snames = new ArrayList<String>();
                for (SkyManager.Option o : client.sky().options()) {
                    sids.add(o.id);
                    snames.add(o.name);
                }
                content.add(new Choice("Sky", sids, snames, () -> sk.preset, v -> {
                    sk.preset = v;
                    changed();
                }));
                content.add(floatSlider("Brightness", 0.1f, 1f, 0.05f, () -> sk.brightness, v -> sk.brightness = v, "%.2f"));
                content.add(new Toggle("Slow rotation (animated)", () -> sk.rotate, v -> {
                    sk.rotate = v;
                    changed();
                }));
                content.add(floatSlider("Rotation speed", 0f, 5f, 0.1f, () -> sk.rotationSpeed, v -> sk.rotationSpeed = v, "%.1f deg/s"));
                content.add(new Toggle("Darken at night", () -> sk.followDayCycle, v -> {
                    sk.followDayCycle = v;
                    changed();
                }));
                content.add(new ActionButton("Open custom skies folder", false, () -> openFolder(client.sky().customDir())));
                text("Overworld only. Replaces the sun, moon and stars while enabled. Custom skies: PNG 3x2 grid of square faces "
                        + "(north, east, south / west, up, down), 384-3072 px wide. See documentation/SKIES.md.");
                if (client.sky().lastError() != null) {
                    content.add(new Header("Last error: " + client.sky().lastError(), false, Theme.ERR));
                }
                break;
            case EFFECTS:
                final ClientConfig.KillEffects ke = c.killEffects;
                content.add(new Toggle("Kill effects", () -> ke.enabled, v -> {
                    ke.enabled = v;
                    changed();
                }));
                List<String> eids = new ArrayList<String>();
                List<String> enames = new ArrayList<String>();
                for (KillEffectType t : KillEffectType.values()) {
                    eids.add(t.id);
                    enames.add(t.label);
                }
                content.add(new Choice("Effect", eids, enames, () -> ke.effect, v -> {
                    ke.effect = v;
                    changed();
                }));
                content.add(new ColorChoice("Effect colour (helix, colour burst)", () -> ke.color, v -> {
                    ke.color = v;
                    changed();
                }));
                content.add(new ActionButton("Preview effect", true, () -> client.killEffects().preview()));
                content.add(floatSlider("Intensity", 0.25f, 2f, 0.05f, () -> ke.intensity, v -> ke.intensity = v, "%.2fx"));
                content.add(intSlider("Max particles per effect", 0, 200, () -> ke.maxParticles, v -> ke.maxParticles = v));
                content.add(new Toggle("Sounds", () -> ke.sounds, v -> {
                    ke.sounds = v;
                    changed();
                }));
                content.add(floatSlider("Volume", 0f, 1f, 0.05f, () -> ke.volume, v -> ke.volume = v, "%.2f"));
                header("Detection");
                content.add(new Toggle("Detect from health (0 HP)", () -> ke.healthDetection, v -> {
                    ke.healthDetection = v;
                    changed();
                }));
                content.add(new Toggle("Detect from chat kill messages", () -> ke.chatDetection, v -> {
                    ke.chatDetection = v;
                    changed();
                }));
                content.add(new Toggle("Include mobs", () -> ke.includeMobs, v -> {
                    ke.includeMobs = v;
                    changed();
                }));
                text("Kills are only detected for entities you hit in the last few seconds. Chat detection needs a kill message "
                        + "that names both you and the victim; servers that word kills differently may not trigger effects. "
                        + "Effects are visual only and visible to you only.");
                break;
            case PERFORMANCE:
                final FrameStats fs = client.performance().frameStats();
                content.add(new Preview(34, (mc, cx, cy, bw, bh) -> {
                    String s = fs.samples() < 30 ? "Measuring frame times..."
                            : String.format("avg %d FPS   1%% low %d FPS   worst %.1f ms", fs.avgFps(), fs.onePercentLowFps(), fs.maxFrameMs());
                    mc.fontRendererObj.drawString(s, cx - mc.fontRendererObj.getStringWidth(s) / 2, cy - 4, Theme.TEXT);
                }));
                header("Presets");
                for (final Preset p : Preset.values()) {
                    String label = p.label + (p.id.equals(c.activePreset) ? "  (active)" : "");
                    content.add(new ActionButton(label, p.id.equals(c.activePreset), () -> {
                        client.applyPreset(p);
                        build();
                    }));
                }
                content.add(new Toggle("Also apply video settings", () -> c.performance.applyVanillaSettings, v -> {
                    c.performance.applyVanillaSettings = v;
                    changed();
                }));
                text("When on, presets also set render distance, particles, graphics and entity shadows "
                        + "(Potato 6/minimal/fast/off, Competitive 8/decreased/fast/off, Visual 12/all/fancy/on).");
                header("Client overhead");
                content.add(intSlider("HUD refresh interval (ms)", 16, 500, () -> c.performance.hudRefreshMs, v -> c.performance.hudRefreshMs = v));
                content.add(new Toggle("Custom sky", () -> c.sky.enabled, v -> {
                    c.sky.enabled = v;
                    changed();
                }));
                content.add(new Toggle("Capes", () -> c.cosmetics.capesEnabled, v -> {
                    c.cosmetics.capesEnabled = v;
                    changed();
                }));
                content.add(new Toggle("Kill effects", () -> c.killEffects.enabled, v -> {
                    c.killEffects.enabled = v;
                    changed();
                }));
                text("Render distance, particles and other vanilla options remain in Options > Video Settings.");
                break;
            default:
                break;
        }
        layout();
    }

    private boolean allHud(boolean background) {
        for (HudElement e : client.hud().elements()) {
            if (background ? !e.cfg().background : !e.cfg().textShadow) return false;
        }
        return true;
    }

    private interface IntGet {
        int get();
    }

    private interface IntSet {
        void set(int v);
    }

    private Slider intSlider(String label, int min, int max, IntGet get, IntSet set) {
        return new Slider(label, min, max, 1, () -> (float) get.get(), v -> {
            set.set(Math.round(v));
            changed();
        }, v -> String.valueOf(Math.round(v)));
    }

    private interface FloatGet {
        float get();
    }

    private interface FloatSet {
        void set(float v);
    }

    private Slider floatSlider(String label, float min, float max, float step, FloatGet get, FloatSet set, String fmt) {
        return new Slider(label, min, max, step, get::get, v -> {
            set.set(v);
            changed();
        }, v -> String.format(fmt, v));
    }

    private static List<String> list(String... s) {
        List<String> l = new ArrayList<String>(s.length);
        for (String x : s) l.add(x);
        return l;
    }

    private void header(String s) {
        content.add(new Header(s, true));
    }

    private void text(String s) {
        content.add(new Header(s, false));
    }

    private void layout() {
        int yy = 0;
        int innerW = contentW - 6; // room for scrollbar
        for (Widget w : content) {
            w.x = contentX;
            w.w = innerW;
            if (w instanceof Header) ((Header) w).layout(mc, innerW);
            w.y = contentY + yy - scroll;
            yy += w.h + 3;
        }
        contentHeight = yy;
        int maxScroll = Math.max(0, contentHeight - contentH);
        if (scroll > maxScroll) {
            scroll = maxScroll;
            layout();
        }
    }

    private void drawCapePreview(Minecraft mc, int cx, int cy) {
        ResourceLocation tex = client.capes().currentTexture();
        if (tex == null) {
            String s = cfg().cosmetics.capesEnabled ? "No cape selected" : "Capes hidden";
            if (CapeManager.NONE.equals(cfg().cosmetics.cape) || !cfg().cosmetics.capesEnabled) {
                mc.fontRendererObj.drawString(s, cx - mc.fontRendererObj.getStringWidth(s) / 2, cy - 4, Theme.MUTED);
            } else {
                String l = "Loading...";
                mc.fontRendererObj.drawString(l, cx - mc.fontRendererObj.getStringWidth(l) / 2, cy - 4, Theme.MUTED);
            }
            return;
        }
        // Outer face of the cape: u=1..11, v=1..17 in the 64x32 layout (10x16 texels).
        GlStateManager.color(1, 1, 1, 1);
        GlStateManager.enableBlend();
        mc.getTextureManager().bindTexture(tex);
        int w = 30;
        int h = 48;
        Gui.drawScaledCustomSizeModalRect(cx - w / 2, cy - h / 2, 1, 1, 10, 16, w, h, 64, 32);
        GlStateManager.disableBlend();
    }

    private void openFolder(java.io.File dir) {
        try {
            if (!dir.isDirectory()) dir.mkdirs();
            java.awt.Desktop.getDesktop().open(dir);
        } catch (Exception e) {
            com.kestrel.KestrelMod.LOG.warn("[Kestrel] cannot open {}: {}", dir, e.toString());
        }
    }

    @Override
    public void drawScreen(int mouseX, int mouseY, float partialTicks) {
        RenderUtil.rect(0, 0, width, height, Theme.BACKDROP);
        RenderUtil.softRect(px, py, px + pw, py + ph, Theme.PANEL);
        RenderUtil.softRect(px, py, px + sideW, py + ph, Theme.SIDEBAR);
        fontRendererObj.drawString("KESTREL", px + 8, py + 10, Theme.ACCENT, false);
        fontRendererObj.drawString(category.label, contentX, py + 10, Theme.TEXT, false);
        String preset = "Preset: " + (Preset.byId(cfg().activePreset) != null ? Preset.byId(cfg().activePreset).label : "-");
        fontRendererObj.drawString(preset, px + pw - fontRendererObj.getStringWidth(preset) - 10, py + 10, Theme.MUTED, false);

        for (int i = 0; i < tabs.size(); i++) {
            ActionButton b = tabs.get(i);
            b.draw(mc, mouseX, mouseY);
            if (Category.values()[i] == category) {
                RenderUtil.rect(b.x, b.y + 2, b.x + 2, b.y + b.h - 2, Theme.ACCENT);
            }
        }

        // Clip content to its area with scissor (GL pixels, origin bottom-left).
        int sf = new net.minecraft.client.gui.ScaledResolution(mc).getScaleFactor();
        GL11.glEnable(GL11.GL_SCISSOR_TEST);
        GL11.glScissor(contentX * sf, mc.displayHeight - (contentY + contentH) * sf, contentW * sf, contentH * sf);
        boolean inside = mouseX >= contentX && mouseX < contentX + contentW && mouseY >= contentY && mouseY < contentY + contentH;
        for (Widget w : content) {
            if (w.y + w.h < contentY || w.y > contentY + contentH) continue;
            w.draw(mc, inside ? mouseX : -1, inside ? mouseY : -1);
        }
        GL11.glDisable(GL11.GL_SCISSOR_TEST);

        if (contentHeight > contentH) {
            int barH = Math.max(16, contentH * contentH / contentHeight);
            int maxScroll = contentHeight - contentH;
            int barY = contentY + (contentH - barH) * scroll / maxScroll;
            RenderUtil.rect(contentX + contentW - 3, contentY, contentX + contentW - 1, contentY + contentH, Theme.TRACK);
            RenderUtil.rect(contentX + contentW - 3, barY, contentX + contentW - 1, barY + barH, Theme.ACCENT_DIM);
        }
        super.drawScreen(mouseX, mouseY, partialTicks);
    }

    @Override
    protected void mouseClicked(int mouseX, int mouseY, int button) throws IOException {
        for (ActionButton b : tabs) {
            if (b.mouseClicked(mouseX, mouseY, button)) return;
        }
        if (mouseX >= contentX && mouseX < contentX + contentW && mouseY >= contentY && mouseY < contentY + contentH) {
            // Copy: a click may rebuild the content list.
            for (Widget w : new ArrayList<Widget>(content)) {
                if (w.mouseClicked(mouseX, mouseY, button)) {
                    dragging = w;
                    return;
                }
            }
        }
        super.mouseClicked(mouseX, mouseY, button);
    }

    @Override
    protected void mouseClickMove(int mouseX, int mouseY, int button, long time) {
        if (dragging != null) dragging.mouseDragged(mouseX, mouseY);
    }

    @Override
    protected void mouseReleased(int mouseX, int mouseY, int state) {
        if (dragging != null) {
            dragging.mouseReleased(mouseX, mouseY);
            dragging = null;
        }
        super.mouseReleased(mouseX, mouseY, state);
    }

    @Override
    public void handleMouseInput() throws IOException {
        super.handleMouseInput();
        int wheel = Mouse.getEventDWheel();
        if (wheel != 0) {
            scroll -= Integer.signum(wheel) * 20;
            scroll = Math.max(0, scroll);
            layout();
        }
    }

    @Override
    public void onGuiClosed() {
        client.configManager().markDirty();
    }

    @Override
    public boolean doesGuiPauseGame() {
        return false; // lets kill-effect previews and sky rotation animate
    }
}
