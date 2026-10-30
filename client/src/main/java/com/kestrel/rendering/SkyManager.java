package com.kestrel.rendering;

import com.kestrel.KestrelClient;
import com.kestrel.KestrelMod;
import com.kestrel.config.ClientConfig;
import com.kestrel.util.AsyncLoader;
import com.kestrel.util.ImageValidation;
import com.kestrel.util.TextureLoader;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.WorldClient;
import net.minecraft.util.ResourceLocation;
import net.minecraftforge.client.IRenderHandler;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;

import java.awt.image.BufferedImage;
import java.io.File;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.Callable;

/**
 * Custom sky selection. When disabled, no sky renderer is installed and
 * vanilla's sky code runs untouched (zero overhead). Applies to the
 * Overworld only; the Nether and End keep their own skies.
 */
public final class SkyManager {
    public static final String CUSTOM_PREFIX = "custom:";
    static final long MAX_FILE_BYTES = 16L * 1024 * 1024;

    private static final String CATALOG_PATH = "/assets/kestrel/skies.json";

    public static final class Option {
        public final String id;
        public final String name;
        /** "custom" or a catalog category ({@link SkyCatalog#CATEGORIES}). */
        public final String category;
        /** Thumbnail path under textures/ (bundled skies), or null for custom skies. */
        public final String thumbnail;

        Option(String id, String name, String category, String thumbnail) {
            this.id = id;
            this.name = name;
            this.category = category;
            this.thumbnail = thumbnail;
        }
    }

    private final SkyCatalog catalog = loadCatalog();

    private final KestrelClient client;
    private final File customDir;
    private final SkyboxRenderer renderer = new SkyboxRenderer(this);
    private volatile List<String> customFiles = Collections.emptyList();

    private String loadedId;
    private String loadingId;
    private ResourceLocation texture;
    private boolean textureIsDynamic;
    private WorldClient appliedWorld;
    private boolean appliedState;
    private String lastError;

    public SkyManager(KestrelClient client, File customDir) {
        this.client = client;
        this.customDir = customDir;
        if (!customDir.isDirectory() && !customDir.mkdirs()) {
            KestrelMod.LOG.warn("[Kestrel] cannot create {}", customDir);
        }
    }

    private static SkyCatalog loadCatalog() {
        try {
            java.io.InputStream in = SkyManager.class.getResourceAsStream(CATALOG_PATH);
            if (in == null) throw new java.io.IOException("missing " + CATALOG_PATH);
            try {
                SkyCatalog c = SkyCatalog.parse(new String(org.apache.commons.io.IOUtils.toByteArray(in), java.nio.charset.StandardCharsets.UTF_8));
                KestrelMod.LOG.info("[Kestrel] sky catalog: {} sky(ies)", c.entries().size());
                return c;
            } finally {
                in.close();
            }
        } catch (Exception e) {
            // A broken catalog must not break the game; only bundled skies are lost.
            KestrelMod.LOG.error("[Kestrel] cannot load sky catalog: {}", e.toString());
            return SkyCatalog.empty();
        }
    }

    public SkyCatalog catalog() {
        return catalog;
    }

    ClientConfig.Sky config() {
        return client.config().sky;
    }

    ResourceLocation texture() {
        return texture;
    }

    public String lastError() {
        return lastError;
    }

    public File customDir() {
        return customDir;
    }

    public List<Option> options() {
        List<Option> out = new ArrayList<Option>();
        for (SkyCatalog.Entry e : catalog.entries()) out.add(new Option(e.id, e.name, e.category, e.thumbnail()));
        for (String f : customFiles) out.add(new Option(CUSTOM_PREFIX + f, f, "custom", null));
        return out;
    }

    /** Rescan custom skies and (re)load the selection if it changed. */
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
        apply();
    }

    /** Load/unload according to the config. Cheap when nothing changed. */
    public void apply() {
        ClientConfig.Sky cfg = config();
        if (!cfg.enabled) {
            if (texture != null || loadingId != null) {
                loadingId = null;
                unload();
            }
        } else if (cfg.preset.equals(loadedId)) {
            loadingId = null; // selection reverted to what is loaded; drop stale loads
        } else if (!cfg.preset.equals(loadingId)) {
            load(cfg.preset);
        }
    }

    private void load(final String id) {
        lastError = null;
        if (!id.startsWith(CUSTOM_PREFIX)) {
            boolean known = catalog.get(id) != null;
            ResourceLocation loc = new ResourceLocation(KestrelMod.MODID, "textures/sky/" + id + ".png");
            if (!known || !TextureLoader.resourceExists(loc)) {
                fail(id, "sky texture not found");
                return;
            }
            loadingId = null; // cancel any in-flight custom load
            unload();
            texture = loc; // vanilla TextureManager loads and caches it on first bind
            textureIsDynamic = false;
            loadedId = id;
            return;
        }
        final String name = id.substring(CUSTOM_PREFIX.length());
        if (name.contains("/") || name.contains("\\") || name.contains("..")) {
            fail(id, "invalid sky file name");
            return;
        }
        final File file = new File(customDir, name);
        loadingId = id;
        client.loader().submit(new Callable<BufferedImage>() {
            @Override
            public BufferedImage call() throws Exception {
                ImageValidation.validateSky(ImageValidation.readPngHeader(file, MAX_FILE_BYTES));
                return TextureLoader.decodePng(file);
            }
        }, new AsyncLoader.Callback<BufferedImage>() {
            @Override
            public void done(BufferedImage img, Throwable error) {
                if (!id.equals(loadingId)) return;
                loadingId = null;
                if (error != null) {
                    fail(id, error.getMessage());
                    return;
                }
                unload();
                texture = TextureLoader.upload("kestrel_sky", img);
                textureIsDynamic = true;
                loadedId = id;
                appliedWorld = null;
                KestrelMod.LOG.info("[Kestrel] loaded sky {}", id);
            }
        });
    }

    private void fail(String id, String msg) {
        lastError = msg;
        loadingId = null;
        KestrelMod.LOG.warn("[Kestrel] sky {} not loaded: {}", id, msg);
    }

    private void unload() {
        if (texture != null && textureIsDynamic) {
            TextureLoader.delete(texture);
        }
        texture = null;
        loadedId = null;
        appliedWorld = null;
    }

    @SubscribeEvent
    public void onTick(TickEvent.ClientTickEvent event) {
        if (event.phase != TickEvent.Phase.END) return;
        WorldClient world = Minecraft.getMinecraft().theWorld;
        boolean want = config().enabled && texture != null;
        if (world == appliedWorld && want == appliedState) return;
        appliedWorld = world;
        appliedState = want;
        if (world == null || world.provider.getDimensionId() != 0) return;
        IRenderHandler current = world.provider.getSkyRenderer();
        if (want) {
            if (current == null || current == renderer) {
                world.provider.setSkyRenderer(renderer);
            } else {
                // Another mod owns the sky; do not fight it.
                lastError = "another mod provides a custom sky renderer";
            }
        } else if (current == renderer) {
            world.provider.setSkyRenderer(null);
        }
    }
}
