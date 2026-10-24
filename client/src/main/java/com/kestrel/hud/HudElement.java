package com.kestrel.hud;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import net.minecraft.client.Minecraft;

/**
 * A movable, scalable HUD module. Subclasses render at (0,0) in unscaled
 * GUI units; the manager applies position and scale. Content is rebuilt by
 * {@link #refresh} at the configured interval rather than every frame.
 */
public abstract class HudElement {
    public final String id;
    public final String name;
    protected final KestrelClient client;
    private final boolean defaultEnabled;
    private final float defaultX;
    private final float defaultY;
    private long lastRefreshMs;

    protected HudElement(KestrelClient client, String id, String name, boolean defaultEnabled, float defaultX, float defaultY) {
        this.client = client;
        this.id = id;
        this.name = name;
        this.defaultEnabled = defaultEnabled;
        this.defaultX = defaultX;
        this.defaultY = defaultY;
    }

    public ClientConfig.HudElementConfig cfg() {
        return client.config().hudElement(id, defaultEnabled, defaultX, defaultY);
    }

    public void resetPosition() {
        ClientConfig.HudElementConfig c = cfg();
        c.x = defaultX;
        c.y = defaultY;
        c.scale = 1.0f;
    }

    /** Rebuild cached content if the refresh interval elapsed (or forced). */
    final void maybeRefresh(Minecraft mc, long nowMs, int intervalMs, boolean force) {
        if (force || nowMs - lastRefreshMs >= intervalMs) {
            lastRefreshMs = nowMs;
            refresh(mc);
        }
    }

    final void invalidate() {
        lastRefreshMs = 0;
    }

    /** Whether there is anything to show in-game right now. */
    protected boolean hasContent(Minecraft mc) {
        return mc.thePlayer != null;
    }

    protected abstract void refresh(Minecraft mc);

    /** Unscaled width/height of the current content (or preview content). */
    public abstract int width(boolean preview);

    public abstract int height(boolean preview);

    /**
     * Draw at the origin. {@code preview} is true in the HUD editor, where
     * elements show sample content if they have nothing real to display.
     */
    protected abstract void render(Minecraft mc, boolean preview);

    /** Elements that want live (per-frame) content, e.g. keystrokes. */
    public boolean realtime() {
        return false;
    }
}
