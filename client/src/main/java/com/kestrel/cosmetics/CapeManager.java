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
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;
import org.apache.commons.io.IOUtils;

import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.io.ByteArrayInputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.Callable;

/**
 * Cape catalog, selection, loading and texture caching.
 *
 * <p>Capes come from the bundled catalog ({@code assets/kestrel/capes.json},
 * read from the mod jar so resource packs cannot change it) or from PNG/GIF
 * files in the custom capes folder. Your selected cape is drawn on your own
 * player. Restricted catalog capes are additionally drawn on the players whose
 * UUID the catalog assigns them to, as seen by anyone running Kestrel.
 *
 * <p>Threading: everything here runs on the game thread except decoding,
 * which runs on the shared loader thread. Decoded frames are uploaded at most
 * {@link #UPLOADS_PER_TICK} per tick. Each loaded cape is cached once and
 * shared by every player wearing it; unused textures are freed.
 */
public final class CapeManager {
    public static final String NONE = "none";
    public static final String CUSTOM_PREFIX = "custom:";
    static final long MAX_FILE_BYTES = 2L * 1024 * 1024;
    /** Upper bound on decoded texels kept per cape (48 MB of ARGB). */
    static final long MAX_TEXELS_PER_CAPE = 12L * 1024 * 1024;
    private static final int UPLOADS_PER_TICK = 4;
    /** Capes shown only on other players are kept this long after last being drawn. */
    private static final long OTHER_PLAYER_KEEP_MS = 30_000L;
    /** Default layout for custom GIFs that are not already cape sheets. */
    private static final int CUSTOM_GIF_SCALE = 8;
    private static final String CATALOG_PATH = "/assets/kestrel/capes.json";
    private static final String TEXTURE_ROOT = "/assets/kestrel/textures/";

    public static final class Option {
        public final String id;
        public final String name;

        Option(String id, String name) {
            this.id = id;
            this.name = name;
        }
    }

    /** One cape's textures, shared by every player wearing it. */
    private static final class Loaded {
        ResourceLocation[] frames;
        FrameTimeline timeline;
        /** Decoded frames waiting for upload (game thread). */
        BufferedImage[] pending;
        int uploaded;
        String error;
        /** Last time this cape was drawn on another player (0 = never). */
        long usedByOthersMs;

        boolean ready() {
            return frames != null && pending == null;
        }
    }

    /** Result of a background decode. */
    private static final class Decoded {
        final BufferedImage[] frames;
        final FrameTimeline timeline;
        final String warning;

        Decoded(BufferedImage[] frames, FrameTimeline timeline, String warning) {
            this.frames = frames;
            this.timeline = timeline;
            this.warning = warning;
        }
    }

    private final KestrelClient client;
    private final File customDir;
    private final CapeCatalog catalog;
    private final Map<String, Loaded> cache = new HashMap<String, Loaded>();
    private volatile List<String> customFiles = Collections.emptyList();
    private String localUuid;
    private boolean localUuidResolved;
    private String restrictionError;

    public CapeManager(KestrelClient client, File customDir) {
        this.client = client;
        this.customDir = customDir;
        if (!customDir.isDirectory() && !customDir.mkdirs()) {
            KestrelMod.LOG.warn("[Kestrel] cannot create {}", customDir);
        }
        this.catalog = loadCatalog();
    }

    private static CapeCatalog loadCatalog() {
        try {
            byte[] json = readBundled(CATALOG_PATH, 256 * 1024);
            CapeCatalog c = CapeCatalog.parse(new String(json, StandardCharsets.UTF_8));
            KestrelMod.LOG.info("[Kestrel] cape catalog: {} cape(s)", c.entries().size());
            return c;
        } catch (Exception e) {
            // A broken catalog must not break the game; only bundled capes are lost.
            KestrelMod.LOG.error("[Kestrel] cannot load cape catalog: {}", e.toString());
            return CapeCatalog.empty();
        }
    }

    public void registerRenderer(CapeRenderer renderer) {
        MinecraftForge.EVENT_BUS.register(renderer);
    }

    public CapeCatalog catalog() {
        return catalog;
    }

    /** Capes the local player may choose: unrestricted ones plus restricted ones they own. */
    public List<Option> options() {
        List<Option> out = new ArrayList<Option>();
        out.add(new Option(NONE, "None"));
        String me = localUuid();
        for (CapeCatalog.Entry e : catalog.entries()) {
            if (e.allows(me)) out.add(new Option(e.id, e.name));
        }
        for (String f : customFiles) out.add(new Option(CUSTOM_PREFIX + f, "Custom: " + f));
        return out;
    }

    public File customDir() {
        return customDir;
    }

    /** Why the selected cape is not shown, or null. */
    public String lastError() {
        if (restrictionError != null) return restrictionError;
        Loaded l = cache.get(client.config().cosmetics.cape);
        return l == null ? null : l.error;
    }

    /** Re-scan the custom folder (off-thread) and retry capes that failed to load. */
    public void reload() {
        client.loader().submit(new Callable<List<String>>() {
            @Override
            public List<String> call() {
                String[] names = customDir.list();
                if (names == null) return Collections.<String>emptyList();
                List<String> files = new ArrayList<String>();
                for (String n : names) {
                    String lower = n.toLowerCase(Locale.ROOT);
                    if ((lower.endsWith(".png") || lower.endsWith(".gif")) && new File(customDir, n).isFile()) files.add(n);
                }
                Collections.sort(files);
                return files;
            }
        }, new AsyncLoader.Callback<List<String>>() {
            @Override
            public void done(List<String> result, Throwable error) {
                if (result != null) customFiles = result;
            }
        });
        // Failed loads get another try (e.g. a file that was fixed or added).
        for (Iterator<Map.Entry<String, Loaded>> it = cache.entrySet().iterator(); it.hasNext(); ) {
            Map.Entry<String, Loaded> e = it.next();
            if (e.getValue().error != null) {
                free(e.getValue());
                it.remove();
            }
        }
        ensureLoaded();
    }

    /** Start loading the selected cape if needed and free textures nobody uses. Cheap. */
    public void ensureLoaded() {
        String want = localCapeId();
        if (want != null && !cache.containsKey(want)) startLoad(want);
        long now = System.currentTimeMillis();
        for (Iterator<Map.Entry<String, Loaded>> it = cache.entrySet().iterator(); it.hasNext(); ) {
            Map.Entry<String, Loaded> e = it.next();
            if (e.getKey().equals(want)) continue;
            if (now - e.getValue().usedByOthersMs > OTHER_PLAYER_KEEP_MS) {
                free(e.getValue());
                it.remove();
            }
        }
    }

    @SubscribeEvent
    public void onClientTick(TickEvent.ClientTickEvent event) {
        if (event.phase != TickEvent.Phase.END) return;
        int budget = UPLOADS_PER_TICK;
        for (Loaded l : cache.values()) {
            if (l.pending == null) continue;
            while (budget > 0 && l.uploaded < l.pending.length) {
                l.frames[l.uploaded] = TextureLoader.upload("kestrel_cape", l.pending[l.uploaded]);
                l.pending[l.uploaded] = null; // let the decoded copy go as soon as possible
                l.uploaded++;
                budget--;
            }
            if (l.uploaded == l.pending.length) l.pending = null;
            if (budget == 0) break;
        }
        ensureLoaded(); // frees capes of players no longer in view (a few map entries at most)
    }

    // ---- what to draw -------------------------------------------------------

    /** Current frame of the local player's selected cape, or null. */
    public ResourceLocation currentTexture() {
        String id = localCapeId();
        return id == null ? null : frame(id, false);
    }

    /**
     * Current frame of the cape assigned to another player by the catalog, or
     * null. Authorisation is the player's UUID only; names are never used.
     */
    public ResourceLocation assignedTexture(UUID player) {
        ClientConfig.Cosmetics c = client.config().cosmetics;
        if (!c.showAssignedCapes) return null;
        CapeCatalog.Entry e = catalog.assignedTo(player);
        return e == null ? null : frame(e.id, true);
    }

    /**
     * The cape id to draw on the local player, or null. Restricted capes are
     * refused here (not only hidden in menus) unless the session's UUID owns them.
     */
    private String localCapeId() {
        ClientConfig.Cosmetics c = client.config().cosmetics;
        restrictionError = null;
        if (!c.capesEnabled || NONE.equals(c.cape)) return null;
        if (c.cape.startsWith(CUSTOM_PREFIX)) return c.cape;
        CapeCatalog.Entry e = catalog.get(c.cape);
        if (e == null) {
            restrictionError = "unknown cape " + c.cape;
            return null;
        }
        if (!e.allows(localUuid())) {
            restrictionError = e.name + " is reserved for another account";
            return null;
        }
        return e.id;
    }

    private ResourceLocation frame(String id, boolean otherPlayer) {
        Loaded l = cache.get(id);
        if (l == null) {
            l = startLoad(id);
        }
        if (otherPlayer) l.usedByOthersMs = System.currentTimeMillis();
        if (!l.ready()) return null;
        ClientConfig.Cosmetics c = client.config().cosmetics;
        if (l.frames.length == 1 || !c.animatedCapes) return l.frames[0];
        return l.frames[l.timeline.frameAt(System.currentTimeMillis(), c.animationFps)];
    }

    private String localUuid() {
        if (!localUuidResolved) {
            localUuidResolved = true;
            try {
                localUuid = CapeCatalog.normalizeUuid(Minecraft.getMinecraft().getSession().getPlayerID());
            } catch (RuntimeException e) {
                localUuid = null; // no session: restricted capes stay unavailable
            }
        }
        return localUuid;
    }

    // ---- loading --------------------------------------------------------------

    private Loaded startLoad(final String id) {
        final Loaded entry = new Loaded();
        cache.put(id, entry);
        final Callable<Decoded> task;
        if (id.startsWith(CUSTOM_PREFIX)) {
            String name = id.substring(CUSTOM_PREFIX.length());
            // Only plain file names inside the capes folder are allowed.
            if (name.isEmpty() || name.contains("/") || name.contains("\\") || name.contains("..")) {
                entry.error = "invalid cape file name";
                return entry;
            }
            final File file = new File(customDir, name);
            task = new Callable<Decoded>() {
                @Override
                public Decoded call() throws Exception {
                    return name(file).endsWith(".gif") ? decodeCustomGif(file) : decodeCustomPng(file);
                }
            };
        } else {
            final CapeCatalog.Entry e = catalog.get(id);
            if (e == null) {
                entry.error = "unknown cape " + id;
                return entry;
            }
            task = new Callable<Decoded>() {
                @Override
                public Decoded call() throws Exception {
                    return decodeBundled(e);
                }
            };
        }
        client.loader().submit(task, new AsyncLoader.Callback<Decoded>() {
            @Override
            public void done(Decoded d, Throwable error) {
                if (cache.get(id) != entry) return; // evicted or reloaded meanwhile
                if (error != null) {
                    entry.error = error.getMessage() != null ? error.getMessage() : error.toString();
                    KestrelMod.LOG.warn("[Kestrel] cape {} not loaded: {}", id, entry.error);
                    return;
                }
                if (d.warning != null) KestrelMod.LOG.warn("[Kestrel] cape {}: {}", id, d.warning);
                entry.timeline = d.timeline;
                entry.frames = new ResourceLocation[d.frames.length];
                entry.pending = d.frames;
                entry.uploaded = 0;
                KestrelMod.LOG.info("[Kestrel] loaded cape {} ({} frame(s), {}x{})", id, d.frames.length,
                        d.frames[0].getWidth(), d.frames[0].getHeight());
            }
        });
        return entry;
    }

    private void free(Loaded l) {
        if (l.frames != null) {
            for (ResourceLocation r : l.frames) TextureLoader.delete(r);
        }
        l.frames = null;
        l.pending = null;
    }

    // ---- decoding (loader thread) ---------------------------------------------

    private static Decoded decodeBundled(CapeCatalog.Entry e) throws IOException {
        if (!e.gif) {
            return decodePngStrip(readBundled(TEXTURE_ROOT + e.texture, MAX_FILE_BYTES), e.name);
        }
        try {
            return decodeGif(readBundled(TEXTURE_ROOT + e.texture, GifDecoder.CAPE_LIMITS.maxBytes), e.layout);
        } catch (IOException gifError) {
            if (e.fallback == null) throw gifError;
            // Static fallback so the cape still shows if the animation cannot be used.
            Decoded still = decodePngStrip(readBundled(TEXTURE_ROOT + e.fallback, MAX_FILE_BYTES), e.name);
            return new Decoded(new BufferedImage[]{still.frames[0]}, FrameTimeline.fixed(1),
                    "animation unavailable (" + gifError.getMessage() + "); using the static cape");
        }
    }

    private static Decoded decodeCustomPng(File file) throws IOException {
        try {
            ImageValidation.Dimensions d = ImageValidation.readPngHeader(file, MAX_FILE_BYTES);
            ImageValidation.validateCape(d);
        } catch (ImageValidation.InvalidImageException ex) {
            throw new IOException(ex.getMessage());
        }
        BufferedImage[] frames = splitFrames(TextureLoader.decodePng(file));
        return new Decoded(frames, FrameTimeline.fixed(frames.length), null);
    }

    private static Decoded decodeCustomGif(File file) throws IOException {
        long len = file.length();
        if (len > GifDecoder.CAPE_LIMITS.maxBytes) {
            throw new IOException(file.getName() + " is too large (" + (len / 1024) + " KB, max "
                    + (GifDecoder.CAPE_LIMITS.maxBytes / 1024) + " KB)");
        }
        byte[] data = readFile(file, GifDecoder.CAPE_LIMITS.maxBytes);
        int[] size = GifDecoder.readHeader(data);
        CapeLayout.Spec spec = null;
        if (CapeLayout.isCapeSheet(size[0], size[1])) {
            spec = new CapeLayout.Spec(CapeLayout.Mode.CAPE, size[0] / 64, null, 0, 0);
        }
        return decodeGif(data, spec);
    }

    /**
     * Decode a GIF into cape frames. A null spec fits the picture onto both
     * faces, padded with the colour of the first frame's top-left pixel.
     */
    private static Decoded decodeGif(byte[] data, final CapeLayout.Spec fixedSpec) throws IOException {
        final List<BufferedImage> frames = new ArrayList<BufferedImage>();
        final List<Integer> delays = new ArrayList<Integer>();
        final CapeLayout.Spec[] spec = {fixedSpec};
        final long[] texels = {0};
        GifDecoder.decode(data, GifDecoder.CAPE_LIMITS, new GifDecoder.FrameSink() {
            @Override
            public void frame(int index, BufferedImage canvas, int delayMs) throws IOException {
                if (spec[0] == null) {
                    int fill = canvas.getRGB(0, 0) | 0xFF000000;
                    spec[0] = new CapeLayout.Spec(CapeLayout.Mode.FACES, CUSTOM_GIF_SCALE, null, fill, darken(fill));
                }
                BufferedImage tex = CapeLayout.build(canvas, spec[0]);
                texels[0] += (long) tex.getWidth() * tex.getHeight();
                if (texels[0] > MAX_TEXELS_PER_CAPE) {
                    throw new GifDecoder.InvalidGifException("animated cape is too large (too many frames for its size)");
                }
                frames.add(tex);
                delays.add(delayMs);
            }
        });
        int[] d = new int[delays.size()];
        for (int i = 0; i < d.length; i++) d[i] = delays.get(i);
        return new Decoded(frames.toArray(new BufferedImage[0]), FrameTimeline.ofDelays(d), null);
    }

    private static Decoded decodePngStrip(byte[] png, String name) throws IOException {
        BufferedImage img = ImageIO.read(new ByteArrayInputStream(png));
        if (img == null) throw new IOException("undecodable cape image " + name);
        try {
            ImageValidation.validateCape(new ImageValidation.Dimensions(img.getWidth(), img.getHeight()));
        } catch (ImageValidation.InvalidImageException ex) {
            throw new IOException(ex.getMessage());
        }
        BufferedImage[] frames = splitFrames(img);
        return new Decoded(frames, FrameTimeline.fixed(frames.length), null);
    }

    /** Animated PNG capes stack 2:1 frames vertically. */
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

    private static int darken(int argb) {
        int r = ((argb >> 16) & 0xFF) * 3 / 4;
        int g = ((argb >> 8) & 0xFF) * 3 / 4;
        int b = (argb & 0xFF) * 3 / 4;
        return 0xFF000000 | (r << 16) | (g << 8) | b;
    }

    private static String name(File f) {
        return f.getName().toLowerCase(Locale.ROOT);
    }

    /** Read a resource from the mod jar itself (never from resource packs). */
    private static byte[] readBundled(String path, long maxBytes) throws IOException {
        InputStream in = CapeManager.class.getResourceAsStream(path);
        if (in == null) throw new IOException("bundled file missing: " + path);
        return readLimited(in, maxBytes, path);
    }

    private static byte[] readFile(File f, long maxBytes) throws IOException {
        return readLimited(new FileInputStream(f), maxBytes, f.getName());
    }

    private static byte[] readLimited(InputStream in, long maxBytes, String what) throws IOException {
        try {
            byte[] data = IOUtils.toByteArray(in);
            if (data.length > maxBytes) throw new IOException(what + " is too large");
            return data;
        } finally {
            in.close();
        }
    }
}
