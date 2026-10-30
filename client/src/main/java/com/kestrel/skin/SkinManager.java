package com.kestrel.skin;

import com.kestrel.KestrelClient;
import com.kestrel.KestrelMod;
import com.kestrel.util.AsyncLoader;
import com.kestrel.util.TextureLoader;
import com.mojang.authlib.GameProfile;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.ImageBufferDownload;
import net.minecraft.util.ResourceLocation;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;

import java.awt.image.BufferedImage;
import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.util.UUID;
import java.util.concurrent.Callable;

/**
 * Applies the local skin chosen in the launcher or the in-game Skin screen.
 *
 * <p>Only the local player's own appearance changes, and only in this
 * client: the player entity and its tab-list entry (matched by profile UUID)
 * get the custom texture and arm model through {@link SkinHooks}. Nothing is
 * uploaded or sent to the server, and the Microsoft/Mojang account skin is
 * untouched. With no active skin everything is vanilla.
 *
 * <p>skin.json is checked once a second (a stat on the loader thread, read
 * only when it changes), so a skin applied in the launcher shows up within a
 * second while playing. Images are decoded and converted off the game thread
 * and uploaded on it.
 */
public final class SkinManager {
    private static final int CHECK_INTERVAL_TICKS = 20;
    private static final String TEXTURE_NAME = "kestrel_local_skin";

    private final KestrelClient client;
    private final SkinStore store;
    private SkinStore.State state = new SkinStore.State();
    private int ticks;
    private boolean checking;
    /** Written by the loader thread and the game thread. */
    private volatile long seenModified = -1;
    private volatile long seenLength = -1;

    /** Uploaded texture of the active skin and the file it came from. */
    private ResourceLocation texture;
    private String textureFile;
    /** File currently being decoded (to drop stale results). */
    private String loadingFile;
    private UUID sessionId;
    private boolean sessionIdResolved;

    public SkinManager(KestrelClient client, File dataDir) {
        this.client = client;
        this.store = new SkinStore(new File(dataDir, "skins"));
        SkinHooks.setManager(this);
    }

    public SkinStore store() {
        return store;
    }

    /** Current library state (game thread). */
    public SkinStore.State state() {
        return state;
    }

    /** Synchronous initial load (start-up, before anything renders). */
    public void loadNow() {
        File f = store.stateFile();
        seenModified = f.lastModified();
        seenLength = f.length();
        apply(store.load());
    }

    /**
     * Save a state changed in game (Skin screen) and apply it at once.
     * Game thread; the write is small and atomic.
     */
    public void saveAndApply(SkinStore.State s) throws IOException {
        store.save(s);
        File f = store.stateFile();
        seenModified = f.lastModified();
        seenLength = f.length();
        apply(store.load());
    }

    @SubscribeEvent
    public void onTick(TickEvent.ClientTickEvent event) {
        if (event.phase != TickEvent.Phase.END || checking || ++ticks < CHECK_INTERVAL_TICKS) return;
        ticks = 0;
        checking = true;
        client.loader().submit(new Callable<SkinStore.State>() {
            @Override
            public SkinStore.State call() {
                File f = store.stateFile();
                long m = f.lastModified();
                long l = f.length();
                if (m == seenModified && l == seenLength) return null;
                seenModified = m;
                seenLength = l;
                return store.load();
            }
        }, new AsyncLoader.Callback<SkinStore.State>() {
            @Override
            public void done(SkinStore.State s, Throwable error) {
                checking = false;
                if (error != null) {
                    KestrelMod.LOG.warn("[Kestrel] cannot read skin.json: {}", error.toString());
                } else if (s != null) {
                    apply(s);
                }
            }
        });
    }

    /** Game thread. */
    private void apply(SkinStore.State s) {
        state = s;
        SkinStore.Entry active = s.activeEntry();
        if (active == null) {
            if (texture != null) KestrelMod.LOG.info("[Kestrel] local skin off (account skin)");
            releaseTexture();
            loadingFile = null;
            return;
        }
        if (active.file.equals(textureFile) || active.file.equals(loadingFile)) return;
        final String file = active.file;
        final File path = store.skinFile(file);
        loadingFile = file;
        client.loader().submit(new Callable<BufferedImage>() {
            @Override
            public BufferedImage call() throws Exception {
                return toGameTexture(SkinStore.validate(Files.readAllBytes(path.toPath())).image);
            }
        }, new AsyncLoader.Callback<BufferedImage>() {
            @Override
            public void done(BufferedImage img, Throwable error) {
                if (!file.equals(loadingFile)) return; // superseded
                loadingFile = null;
                if (error != null) {
                    KestrelMod.LOG.warn("[Kestrel] cannot load skin {}: {}", file, error.getMessage());
                    releaseTexture();
                    return;
                }
                releaseTexture();
                texture = TextureLoader.upload(TEXTURE_NAME, img);
                textureFile = file;
                KestrelMod.LOG.info("[Kestrel] local skin {} applied", file);
            }
        });
    }

    /**
     * Convert exactly as the game converts downloaded skins: legacy 64x32
     * skins get mirrored left limbs, the base layer is made opaque and the
     * unused hat area cleared ({@code ImageBufferDownload}).
     */
    static BufferedImage toGameTexture(BufferedImage img) {
        return new ImageBufferDownload().parseUserSkin(img);
    }

    private void releaseTexture() {
        if (texture != null) {
            TextureLoader.delete(texture);
        }
        texture = null;
        textureFile = null;
    }

    // ---- queried by the mixins (render thread) ---------------------------

    /** The custom skin for this profile, or null for vanilla behaviour. */
    ResourceLocation skinFor(GameProfile profile) {
        return texture != null && isSelf(profile) ? texture : null;
    }

    /** "default" or "slim" (vanilla's names), or null for vanilla behaviour. */
    String modelFor(GameProfile profile) {
        if (texture == null || !isSelf(profile)) return null;
        SkinStore.Entry e = state.find(textureFile);
        return e != null && SkinStore.MODEL_SLIM.equals(e.model) ? "slim" : "default";
    }

    private boolean isSelf(GameProfile profile) {
        if (profile == null || profile.getId() == null) return false;
        Minecraft mc = Minecraft.getMinecraft();
        if (mc.thePlayer != null && profile.getId().equals(mc.thePlayer.getGameProfile().getId())) return true;
        return profile.getId().equals(sessionId());
    }

    private UUID sessionId() {
        if (!sessionIdResolved) {
            sessionIdResolved = true;
            try {
                sessionId = Minecraft.getMinecraft().getSession().getProfile().getId();
            } catch (RuntimeException e) {
                sessionId = null; // malformed session UUID: match the player entity only
            }
        }
        return sessionId;
    }
}
