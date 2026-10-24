package com.kestrel.cosmetics;

import com.kestrel.KestrelClient;
import com.kestrel.KestrelMod;
import com.kestrel.config.ClientConfig;
import com.kestrel.util.AsyncLoader;
import com.kestrel.util.ImageValidation;
import com.kestrel.util.TextureLoader;
import net.minecraft.client.Minecraft;
import net.minecraft.util.ResourceLocation;
import net.minecraftforge.common.MinecraftForge;
import org.apache.commons.io.IOUtils;

import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.io.ByteArrayInputStream;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.Callable;

/**
 * Cape selection, loading and caching.
 *
 * <p>Capes are drawn on your own player only, by this client. Other players
 * (vanilla or other clients) do not see them.
 *
 * <p>Images are validated (type, size, dimensions) and decoded on the loader
 * thread, then uploaded on the game thread. Only the selected cape is kept
 * in GPU memory; switching frees the previous textures.
 */
public final class CapeManager {
    public static final String NONE = "none";
    public static final String CUSTOM_PREFIX = "custom:";
    static final long MAX_FILE_BYTES = 2L * 1024 * 1024;

    /** Bundled presets: id, display name. */
    private static final String[][] BUILTIN = {
            {"ember", "Ember"},
            {"glacier", "Glacier"},
            {"midnight", "Midnight"},
            {"verdant", "Verdant"},
            {"aurora", "Aurora (animated)"},
    };

    public static final class Option {
        public final String id;
        public final String name;

        Option(String id, String name) {
            this.id = id;
            this.name = name;
        }
    }

    private final KestrelClient client;
    private final File customDir;
    private volatile List<String> customFiles = Collections.emptyList();

    private String loadedId = NONE;
    private String loadingId;
    private ResourceLocation[] frames;
    private String lastError;

    public CapeManager(KestrelClient client, File customDir) {
        this.client = client;
        this.customDir = customDir;
        if (!customDir.isDirectory() && !customDir.mkdirs()) {
            KestrelMod.LOG.warn("[Kestrel] cannot create {}", customDir);
        }
    }

    public void registerRenderer(CapeRenderer renderer) {
        MinecraftForge.EVENT_BUS.register(renderer);
    }

    public List<Option> options() {
        List<Option> out = new ArrayList<Option>();
        out.add(new Option(NONE, "None"));
        for (String[] b : BUILTIN) out.add(new Option(b[0], b[1]));
        for (String f : customFiles) out.add(new Option(CUSTOM_PREFIX + f, "Custom: " + f));
        return out;
    }

    public File customDir() {
        return customDir;
    }

    public String lastError() {
        return lastError;
    }

    /** Re-scan the custom folder (off-thread) and load the selected cape if needed. */
    public void reload() {
        client.loader().submit(new Callable<List<String>>() {
            @Override
            public List<String> call() {
                String[] names = customDir.list();
                if (names == null) return Collections.emptyList();
                List<String> pngs = new ArrayList<String>();
                for (String n : names) {
                    if (n.toLowerCase().endsWith(".png") && new File(customDir, n).isFile()) pngs.add(n);
                }
                Collections.sort(pngs);
                return pngs;
            }
        }, new AsyncLoader.Callback<List<String>>() {
            @Override
            public void done(List<String> result, Throwable error) {
                if (result != null) customFiles = result;
            }
        });
        ensureLoaded();
    }

    /** Current frame texture for the local player, or null for no cape. */
    public ResourceLocation currentTexture() {
        ClientConfig.Cosmetics c = client.config().cosmetics;
        if (!c.capesEnabled || frames == null || frames.length == 0) return null;
        if (!c.cape.equals(loadedId)) return null;
        if (frames.length == 1 || !c.animatedCapes) return frames[0];
        long frameMs = 1000L / c.animationFps;
        return frames[(int) ((System.currentTimeMillis() / frameMs) % frames.length)];
    }

    /** Load the selected cape if it is not already loaded. Cheap when nothing changed. */
    public void ensureLoaded() {
        ClientConfig.Cosmetics c = client.config().cosmetics;
        final String want = c.capesEnabled ? c.cape : NONE;
        if (want.equals(loadingId)) return;
        if (want.equals(loadedId)) {
            loadingId = null; // a newer selection reverted to the loaded cape
            return;
        }
        if (NONE.equals(want)) {
            loadingId = null; // cancel any in-flight load
            unload();
            return;
        }
        loadingId = want;
        lastError = null;

        // Bundled textures are read on the game thread (resource manager is
        // not documented as thread-safe); files are read on the loader thread.
        final byte[] bundled;
        final File file;
        if (want.startsWith(CUSTOM_PREFIX)) {
            String name = want.substring(CUSTOM_PREFIX.length());
            // Only plain file names inside the capes folder are allowed.
            if (name.contains("/") || name.contains("\\") || name.contains("..")) {
                fail(want, "invalid cape file name");
                return;
            }
            file = new File(customDir, name);
            bundled = null;
        } else {
            if (!isBuiltin(want)) {
                fail(want, "unknown cape " + want);
                return;
            }
            file = null;
            try {
                bundled = readResource(new ResourceLocation(KestrelMod.MODID, "textures/capes/" + want + ".png"));
            } catch (IOException e) {
                fail(want, "bundled cape missing: " + e.getMessage());
                return;
            }
        }

        client.loader().submit(new Callable<BufferedImage[]>() {
            @Override
            public BufferedImage[] call() throws Exception {
                BufferedImage img;
                if (file != null) {
                    ImageValidation.Dimensions d = ImageValidation.readPngHeader(file, MAX_FILE_BYTES);
                    ImageValidation.validateCape(d);
                    img = TextureLoader.decodePng(file);
                } else {
                    img = ImageIO.read(new ByteArrayInputStream(bundled));
                    if (img == null) throw new IOException("undecodable bundled cape");
                    ImageValidation.validateCape(new ImageValidation.Dimensions(img.getWidth(), img.getHeight()));
                }
                return splitFrames(img);
            }
        }, new AsyncLoader.Callback<BufferedImage[]>() {
            @Override
            public void done(BufferedImage[] imgs, Throwable error) {
                if (!want.equals(loadingId)) return; // superseded by a newer selection
                if (error != null) {
                    fail(want, error.getMessage());
                    return;
                }
                unload();
                ResourceLocation[] locs = new ResourceLocation[imgs.length];
                for (int i = 0; i < imgs.length; i++) {
                    locs[i] = TextureLoader.upload("kestrel_cape_" + i, imgs[i]);
                }
                frames = locs;
                loadedId = want;
                loadingId = null;
                KestrelMod.LOG.info("[Kestrel] loaded cape {} ({} frame(s))", want, locs.length);
            }
        });
    }

    private void fail(String id, String msg) {
        lastError = msg;
        loadingId = null;
        KestrelMod.LOG.warn("[Kestrel] cape {} not loaded: {}", id, msg);
    }

    private void unload() {
        if (frames != null) {
            for (ResourceLocation l : frames) TextureLoader.delete(l);
        }
        frames = null;
        loadedId = NONE;
    }

    static boolean isBuiltin(String id) {
        for (String[] b : BUILTIN) if (b[0].equals(id)) return true;
        return false;
    }

    /** Animated capes stack 2:1 frames vertically. */
    static BufferedImage[] splitFrames(BufferedImage img) {
        int frameH = img.getWidth() / 2;
        int n = Math.max(1, img.getHeight() / frameH);
        BufferedImage[] out = new BufferedImage[n];
        for (int i = 0; i < n; i++) {
            BufferedImage f = new BufferedImage(img.getWidth(), frameH, BufferedImage.TYPE_INT_ARGB);
            java.awt.Graphics g = f.getGraphics();
            try {
                g.drawImage(img.getSubimage(0, i * frameH, img.getWidth(), frameH), 0, 0, null);
            } finally {
                g.dispose();
            }
            out[i] = f;
        }
        return out;
    }

    private static byte[] readResource(ResourceLocation loc) throws IOException {
        InputStream in = Minecraft.getMinecraft().getResourceManager().getResource(loc).getInputStream();
        try {
            byte[] data = IOUtils.toByteArray(in);
            if (data.length > MAX_FILE_BYTES) throw new IOException("resource too large");
            return data;
        } finally {
            in.close();
        }
    }

}
