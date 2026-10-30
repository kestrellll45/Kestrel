package com.kestrel.skin;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonParseException;
import com.kestrel.util.ImageValidation;

import javax.imageio.ImageIO;
import javax.imageio.ImageReader;
import javax.imageio.stream.ImageInputStream;
import java.awt.image.BufferedImage;
import java.io.ByteArrayInputStream;
import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Set;

/**
 * The local skin library, shared with the launcher
 * ({@code kestrel_core::skins} in Rust; both follow the same rules).
 *
 * <p>Files live in {@code <gameDir>/kestrel/skins/}: validated skin copies
 * named {@code <16 hex>.png} by content hash, and {@code skin.json} listing
 * them and the active one. Either side rewrites skin.json atomically; the
 * game polls it so changes made in the launcher apply while playing. Pure
 * Java (no Minecraft types) so it is unit-tested.
 */
public final class SkinStore {
    public static final String MODEL_CLASSIC = "classic";
    public static final String MODEL_SLIM = "slim";
    public static final long MAX_SKIN_BYTES = 512 * 1024;
    public static final int MAX_SKINS = 64;
    static final int MAX_SOURCE = 300;
    static final String STATE_FILE = "skin.json";
    /** PNGs dropped here are imported by {@link #importFolder()}. */
    static final String IMPORT_DIR = "import";

    private static final Gson GSON = new GsonBuilder().setPrettyPrinting().serializeNulls().create();

    /** One imported skin (field names match skin.json). */
    public static final class Entry {
        public String file;
        public String name = "";
        public String model = MODEL_CLASSIC;
        public String source = "";
        public long added;

        public Entry copy() {
            Entry e = new Entry();
            e.file = file;
            e.name = name;
            e.model = model;
            e.source = source;
            e.added = added;
            return e;
        }
    }

    /** Contents of skin.json. */
    public static final class State {
        public int version = 1;
        /** File name of the active skin; null = the account skin. */
        public String active;
        public List<Entry> skins = new ArrayList<Entry>();
        public long revision;
        public String updatedBy = "";

        public Entry activeEntry() {
            return find(active);
        }

        public Entry find(String file) {
            if (file == null) return null;
            for (Entry e : skins) {
                if (file.equals(e.file)) return e;
            }
            return null;
        }
    }

    /** A rejected skin; the message is shown to the user. */
    public static final class SkinException extends Exception {
        private static final long serialVersionUID = 1L;

        public SkinException(String message) {
            super(message);
        }
    }

    /** A validated skin image. */
    public static final class Validated {
        public final BufferedImage image;
        public final boolean legacy;
        public final String suggestedModel;

        Validated(BufferedImage image, boolean legacy, String suggestedModel) {
            this.image = image;
            this.legacy = legacy;
            this.suggestedModel = suggestedModel;
        }
    }

    private final File dir;

    public SkinStore(File dir) {
        this.dir = dir;
    }

    public File dir() {
        return dir;
    }

    public File stateFile() {
        return new File(dir, STATE_FILE);
    }

    public File importDir() {
        return new File(dir, IMPORT_DIR);
    }

    /** Skin file for a name from skin.json, or null if the name is not valid. */
    public File skinFile(String name) {
        return isValidFileName(name) ? new File(dir, name) : null;
    }

    // ---- state ---------------------------------------------------------

    /** Read skin.json; a missing or malformed file means "account skin". */
    public State load() {
        State s = null;
        File f = stateFile();
        if (f.isFile() && f.length() <= 256 * 1024) {
            try {
                s = GSON.fromJson(new String(Files.readAllBytes(f.toPath()), StandardCharsets.UTF_8), State.class);
            } catch (IOException e) {
                s = null;
            } catch (JsonParseException e) {
                s = null;
            } catch (RuntimeException e) {
                s = null; // Gson type errors in a hand-edited file
            }
        }
        if (s == null) s = new State();
        normalize(s);
        return s;
    }

    /** Write skin.json atomically with a new, strictly increasing revision. */
    public void save(State s) throws IOException {
        normalize(s);
        State previous = load();
        s.revision = Math.max(Math.max(System.currentTimeMillis(), previous.revision + 1), s.revision + 1);
        s.version = 1;
        s.updatedBy = "game";
        writeAtomically(stateFile(), GSON.toJson(s).getBytes(StandardCharsets.UTF_8));
    }

    /** Repair anything a hand edit or another writer could have left. */
    public void normalize(State s) {
        if (s.skins == null) s.skins = new ArrayList<Entry>();
        Set<String> seen = new HashSet<String>();
        for (Iterator<Entry> it = s.skins.iterator(); it.hasNext(); ) {
            Entry e = it.next();
            if (e == null || !isValidFileName(e.file) || !new File(dir, e.file).isFile() || !seen.add(e.file)) {
                it.remove();
                continue;
            }
            e.model = normalizeModel(e.model);
            e.name = sanitizeName(e.name);
            if (e.name.isEmpty()) e.name = "Skin";
            e.source = e.source == null ? "" : truncate(e.source, MAX_SOURCE);
        }
        while (s.skins.size() > MAX_SKINS) {
            s.skins.remove(s.skins.size() - 1);
        }
        if (s.active != null && s.find(s.active) == null) s.active = null;
        if (s.updatedBy == null) s.updatedBy = "";
    }

    // ---- validation ----------------------------------------------------

    /**
     * Validate skin PNG bytes: size, PNG signature, 64x64 or legacy 64x32
     * (read from the header before any decoding), then decode with the PNG
     * reader only.
     */
    public static Validated validate(byte[] bytes) throws SkinException {
        if (bytes.length > MAX_SKIN_BYTES) {
            throw new SkinException("file is too large (" + bytes.length / 1024 + " KB, max " + MAX_SKIN_BYTES / 1024 + " KB)");
        }
        if (bytes.length < 24) throw new SkinException("not a PNG image (only .png skins are supported)");
        ImageValidation.Dimensions d;
        try {
            byte[] head = new byte[24];
            System.arraycopy(bytes, 0, head, 0, 24);
            d = ImageValidation.parseHeader(head, "skin");
        } catch (ImageValidation.InvalidImageException e) {
            throw new SkinException("not a PNG image (only .png skins are supported)");
        }
        if (d.width != 64 || (d.height != 64 && d.height != 32)) {
            throw new SkinException("a Minecraft skin must be 64x64 (or the old 64x32 format); this image is " + d);
        }
        BufferedImage img;
        try {
            img = decodePng(bytes);
        } catch (IOException e) {
            throw new SkinException("corrupt PNG: " + e.getMessage());
        }
        if (img == null || img.getWidth() != d.width || img.getHeight() != d.height) {
            throw new SkinException("corrupt PNG (header and image size differ)");
        }
        boolean legacy = d.height == 32;
        return new Validated(img, legacy, !legacy && isSlim(img) ? MODEL_SLIM : MODEL_CLASSIC);
    }

    static BufferedImage decodePng(byte[] bytes) throws IOException {
        ImageInputStream in = ImageIO.createImageInputStream(new ByteArrayInputStream(bytes));
        if (in == null) throw new IOException("cannot read image");
        try {
            Iterator<ImageReader> readers = ImageIO.getImageReadersByFormatName("png");
            if (!readers.hasNext()) throw new IOException("no PNG decoder available");
            ImageReader reader = readers.next();
            try {
                reader.setInput(in, true, true);
                return reader.read(0);
            } finally {
                reader.dispose();
            }
        } finally {
            in.close();
        }
    }

    /** Slim arms leave columns 54-55 of the right arm (rows 20-31) empty. */
    static boolean isSlim(BufferedImage img) {
        for (int y = 20; y < 32; y++) {
            if ((img.getRGB(54, y) >>> 24) != 0 || (img.getRGB(55, y) >>> 24) != 0) return false;
        }
        return true;
    }

    // ---- import --------------------------------------------------------

    /**
     * Copy validated bytes into the library (unchanged, so transparency is
     * kept) and add or refresh the entry. Does not change the active skin.
     */
    public Entry importBytes(State s, byte[] bytes, String name, String source, String modelHint) throws SkinException, IOException {
        Validated v = validate(bytes);
        String file = sha1(bytes).substring(0, 16) + ".png";
        if (!dir.isDirectory() && !dir.mkdirs()) throw new IOException("cannot create " + dir);
        File dest = new File(dir, file);
        if (!dest.isFile() || dest.length() != bytes.length) {
            writeAtomically(dest, bytes);
        }
        Entry e = new Entry();
        e.file = file;
        e.name = sanitizeName(name);
        if (e.name.isEmpty()) e.name = "Skin";
        e.model = v.legacy ? MODEL_CLASSIC : (modelHint != null ? normalizeModel(modelHint) : v.suggestedModel);
        e.source = truncate(source == null ? "" : source, MAX_SOURCE);
        e.added = System.currentTimeMillis();
        for (Iterator<Entry> it = s.skins.iterator(); it.hasNext(); ) {
            if (file.equals(it.next().file)) it.remove();
        }
        s.skins.add(0, e);
        while (s.skins.size() > MAX_SKINS) s.skins.remove(s.skins.size() - 1);
        return e;
    }

    /** Import a user-chosen PNG file (the original is never modified). */
    public Entry importFile(State s, File src) throws SkinException, IOException {
        if (!src.isFile()) throw new SkinException(src.getName() + " is not a file");
        if (src.length() > MAX_SKIN_BYTES) {
            throw new SkinException("file is too large (" + src.length() / 1024 + " KB, max " + MAX_SKIN_BYTES / 1024 + " KB)");
        }
        String n = src.getName();
        String stem = n.toLowerCase(Locale.ROOT).endsWith(".png") ? n.substring(0, n.length() - 4) : n;
        return importBytes(s, Files.readAllBytes(src.toPath()), stem, n, null);
    }

    /**
     * Import every PNG in the import folder (for when a file picker is not
     * available). Re-importing is harmless (content-addressed). The files
     * are left where they are.
     *
     * @return problems, one line per rejected file
     */
    public List<String> importFolder(State s) {
        List<String> problems = new ArrayList<String>();
        File[] files = importDir().listFiles();
        if (files == null) return problems;
        int n = 0;
        for (File f : files) {
            if (!f.isFile() || !f.getName().toLowerCase(Locale.ROOT).endsWith(".png")) continue;
            if (++n > MAX_SKINS) break;
            try {
                importFile(s, f);
            } catch (SkinException e) {
                problems.add(f.getName() + ": " + e.getMessage());
            } catch (IOException e) {
                problems.add(f.getName() + ": " + e.getMessage());
            }
        }
        return problems;
    }

    /** Remove an entry and its copy in the library. */
    public void remove(State s, String file) throws IOException {
        File f = skinFile(file);
        if (f == null) throw new IOException("invalid skin name");
        for (Iterator<Entry> it = s.skins.iterator(); it.hasNext(); ) {
            if (file.equals(it.next().file)) it.remove();
        }
        if (file.equals(s.active)) s.active = null;
        if (f.exists()) Files.delete(f.toPath());
    }

    // ---- helpers -------------------------------------------------------

    public static String normalizeModel(String m) {
        return m != null && m.trim().equalsIgnoreCase(MODEL_SLIM) ? MODEL_SLIM : MODEL_CLASSIC;
    }

    public static boolean isValidFileName(String name) {
        if (name == null || name.length() != 20 || !name.endsWith(".png")) return false;
        for (int i = 0; i < 16; i++) {
            char c = name.charAt(i);
            if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'))) return false;
        }
        return true;
    }

    public static String sanitizeName(String name) {
        if (name == null) return "";
        StringBuilder b = new StringBuilder();
        for (int i = 0; i < name.length() && b.length() < 32; i++) {
            char c = name.charAt(i);
            if (c == '\u00a7') {
                i++; // drop Minecraft formatting codes (section sign + code)
            } else if (!Character.isISOControl(c)) {
                b.append(c);
            }
        }
        return b.toString().trim();
    }

    static String truncate(String s, int max) {
        return s.length() <= max ? s : s.substring(0, max);
    }

    static String sha1(byte[] data) {
        try {
            byte[] d = MessageDigest.getInstance("SHA-1").digest(data);
            StringBuilder b = new StringBuilder();
            for (byte x : d) b.append(String.format("%02x", x & 0xFF));
            return b.toString();
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    static void writeAtomically(File dest, byte[] data) throws IOException {
        File parent = dest.getParentFile();
        if (parent != null && !parent.isDirectory() && !parent.mkdirs()) throw new IOException("cannot create " + parent);
        File tmp = new File(dest.getPath() + ".game.tmp"); // the launcher uses .tmp
        Files.write(tmp.toPath(), data);
        try {
            Files.move(tmp.toPath(), dest.toPath(), StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
        } catch (AtomicMoveNotSupportedException e) {
            Files.move(tmp.toPath(), dest.toPath(), StandardCopyOption.REPLACE_EXISTING);
        }
    }
}
