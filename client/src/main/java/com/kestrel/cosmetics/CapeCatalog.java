package com.kestrel.cosmetics;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

/**
 * The bundled cape catalog ({@code assets/kestrel/capes.json}).
 *
 * <p>The catalog ships inside the signed client jar, so it is the trusted
 * source of which capes exist and who may wear restricted ones. A cape with
 * an {@code owners} list is <em>restricted</em>: it is offered to, loaded
 * for, and rendered on only players whose UUID is in that list. Usernames are
 * never consulted. An {@code owners} key that yields no valid UUID restricts
 * the cape to nobody (fail closed). The launcher parses the same file.
 *
 * <p>Pure Java (no Minecraft types) so it is unit-tested.
 */
public final class CapeCatalog {
    /** Matches the 64-px vanilla sheet, as used by the bundled PNG presets. */
    private static final int DEFAULT_SCALE = 8;

    public static final class Entry {
        public final String id;
        public final String name;
        /** Path under {@code assets/kestrel/textures/}, e.g. {@code capes/ember.png}. */
        public final String texture;
        /** Optional static PNG (cape layout) used when the GIF cannot be decoded. */
        public final String fallback;
        public final boolean gif;
        /** Layout for GIF textures (PNG presets always use the cape layout). */
        public final CapeLayout.Spec layout;
        /** Normalised owner UUIDs (32 lower-case hex), or null if unrestricted. */
        private final Set<String> owners;

        Entry(String id, String name, String texture, String fallback, CapeLayout.Spec layout, Set<String> owners) {
            this.id = id;
            this.name = name;
            this.texture = texture;
            this.fallback = fallback;
            this.gif = texture.toLowerCase(Locale.ROOT).endsWith(".gif");
            this.layout = layout;
            this.owners = owners == null ? null : Collections.unmodifiableSet(owners);
        }

        public boolean restricted() {
            return owners != null;
        }

        /** Whether the player with this UUID may wear the cape. Null UUID: only unrestricted capes. */
        public boolean allows(String uuid) {
            if (owners == null) return true;
            String n = normalizeUuid(uuid);
            return n != null && owners.contains(n);
        }

        public Set<String> owners() {
            return owners == null ? Collections.<String>emptySet() : owners;
        }
    }

    private final List<Entry> entries;
    private final Map<String, Entry> byId;
    /** Restricted capes by owner, for rendering on other players. */
    private final Map<UUID, Entry> byOwner;

    private CapeCatalog(List<Entry> entries) {
        this.entries = Collections.unmodifiableList(entries);
        Map<String, Entry> ids = new HashMap<String, Entry>();
        Map<UUID, Entry> owners = new HashMap<UUID, Entry>();
        for (Entry e : entries) {
            ids.put(e.id, e);
            for (String o : e.owners()) {
                UUID u = toUuid(o);
                if (!owners.containsKey(u)) owners.put(u, e); // first listed cape wins
            }
        }
        this.byId = ids;
        this.byOwner = owners;
    }

    public static CapeCatalog empty() {
        return new CapeCatalog(new ArrayList<Entry>());
    }

    public List<Entry> entries() {
        return entries;
    }

    public Entry get(String id) {
        return id == null ? null : byId.get(id);
    }

    /** The restricted cape assigned to this player, or null. No allocation. */
    public Entry assignedTo(UUID uuid) {
        return uuid == null ? null : byOwner.get(uuid);
    }

    /**
     * Parse the catalog. Malformed entries are rejected with an exception so
     * a broken build is caught by tests rather than silently dropping capes.
     */
    public static CapeCatalog parse(String json) {
        JsonElement root = new JsonParser().parse(json);
        if (!root.isJsonObject()) throw new IllegalArgumentException("catalog must be a JSON object");
        JsonElement capes = root.getAsJsonObject().get("capes");
        if (capes == null || !capes.isJsonArray()) throw new IllegalArgumentException("catalog has no capes array");
        List<Entry> out = new ArrayList<Entry>();
        Set<String> seen = new HashSet<String>();
        for (JsonElement el : capes.getAsJsonArray()) {
            if (!el.isJsonObject()) throw new IllegalArgumentException("cape entry must be an object");
            JsonObject o = el.getAsJsonObject();
            String id = reqString(o, "id");
            if (!id.matches("[a-z0-9_]{1,32}") || "none".equals(id)) {
                throw new IllegalArgumentException("invalid cape id: " + id);
            }
            if (!seen.add(id)) throw new IllegalArgumentException("duplicate cape id: " + id);
            String name = reqString(o, "name");
            String texture = safePath(reqString(o, "texture"));
            String fallback = o.has("fallback") ? safePath(reqString(o, "fallback")) : null;
            if (fallback != null && !fallback.endsWith(".png")) {
                throw new IllegalArgumentException(id + ": fallback must be a PNG");
            }
            if (!texture.endsWith(".png") && !texture.endsWith(".gif")) {
                throw new IllegalArgumentException(id + ": texture must be .png or .gif");
            }

            CapeLayout.Mode mode = CapeLayout.Mode.CAPE;
            if (o.has("layout")) {
                String l = reqString(o, "layout");
                if ("faces".equals(l)) mode = CapeLayout.Mode.FACES;
                else if (!"cape".equals(l)) throw new IllegalArgumentException(id + ": unknown layout " + l);
            }
            int scale = o.has("scale") ? o.get("scale").getAsInt() : DEFAULT_SCALE;
            int[] crop = null;
            if (o.has("crop")) {
                JsonArray a = o.get("crop").getAsJsonArray();
                if (a.size() != 4) throw new IllegalArgumentException(id + ": crop must be [x, y, w, h]");
                crop = new int[]{a.get(0).getAsInt(), a.get(1).getAsInt(), a.get(2).getAsInt(), a.get(3).getAsInt()};
            }
            int fill = o.has("fill") ? parseColor(reqString(o, "fill")) : 0xFF000000;
            int edge = o.has("edge") ? parseColor(reqString(o, "edge")) : darken(fill);
            CapeLayout.Spec spec = new CapeLayout.Spec(mode, scale, crop, fill, edge);

            Set<String> owners = null;
            if (o.has("owners")) {
                owners = new LinkedHashSet<String>();
                JsonElement ow = o.get("owners");
                if (!ow.isJsonArray()) throw new IllegalArgumentException(id + ": owners must be an array");
                for (JsonElement u : ow.getAsJsonArray()) {
                    String n = u.isJsonPrimitive() ? normalizeUuid(u.getAsString()) : null;
                    if (n == null) throw new IllegalArgumentException(id + ": invalid owner UUID " + u);
                    owners.add(n);
                }
                // An empty list stays an empty (deny-all) set: fail closed.
            }
            out.add(new Entry(id, name, texture, fallback, spec, owners));
        }
        return new CapeCatalog(out);
    }

    /**
     * Canonical form of a Minecraft UUID: 32 lower-case hex digits, no dashes.
     * Accepts dashed (8-4-4-4-12) or undashed input in any case.
     *
     * @return the normalised UUID, or null if the input is not a UUID
     */
    public static String normalizeUuid(String s) {
        if (s == null) return null;
        String t = s.trim();
        if (t.length() == 36) {
            if (t.charAt(8) != '-' || t.charAt(13) != '-' || t.charAt(18) != '-' || t.charAt(23) != '-') return null;
            t = t.substring(0, 8) + t.substring(9, 13) + t.substring(14, 18) + t.substring(19, 23) + t.substring(24);
        }
        if (t.length() != 32) return null;
        StringBuilder b = new StringBuilder(32);
        for (int i = 0; i < 32; i++) {
            char c = Character.toLowerCase(t.charAt(i));
            if ((c < '0' || c > '9') && (c < 'a' || c > 'f')) return null;
            b.append(c);
        }
        return b.toString();
    }

    static UUID toUuid(String normalized) {
        long hi = Long.parseUnsignedLong(normalized.substring(0, 16), 16);
        long lo = Long.parseUnsignedLong(normalized.substring(16), 16);
        return new UUID(hi, lo);
    }

    static int parseColor(String s) {
        if (s.length() != 7 || s.charAt(0) != '#') throw new IllegalArgumentException("colour must be #RRGGBB: " + s);
        return 0xFF000000 | Integer.parseInt(s.substring(1), 16);
    }

    private static int darken(int argb) {
        int r = ((argb >> 16) & 0xFF) * 3 / 4;
        int g = ((argb >> 8) & 0xFF) * 3 / 4;
        int b = (argb & 0xFF) * 3 / 4;
        return 0xFF000000 | (r << 16) | (g << 8) | b;
    }

    private static String reqString(JsonObject o, String k) {
        JsonElement e = o.get(k);
        if (e == null || !e.isJsonPrimitive() || !e.getAsJsonPrimitive().isString() || e.getAsString().isEmpty()) {
            throw new IllegalArgumentException("missing string field " + k);
        }
        return e.getAsString();
    }

    private static String safePath(String p) {
        if (!p.matches("[a-z0-9_/]+\\.(png|gif)") || p.contains("..") || p.startsWith("/")) {
            throw new IllegalArgumentException("invalid texture path: " + p);
        }
        return p;
    }
}
