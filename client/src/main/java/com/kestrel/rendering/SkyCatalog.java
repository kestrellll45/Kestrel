package com.kestrel.rendering;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * The bundled sky catalog ({@code assets/kestrel/skies.json}): ids, names and
 * menu categories. Atlases are {@code textures/sky/<id>.png}, thumbnails
 * {@code textures/sky/thumbs/<id>.png}. The launcher reads the same file.
 * Pure Java (no Minecraft types) so it is unit-tested.
 */
public final class SkyCatalog {
    /** Menu categories, in display order. */
    public static final String[] CATEGORIES = {"sunsets", "space", "atmospheric", "dark", "neon", "fantasy", "minimal", "blocky", "kestrel"};

    public static final class Entry {
        public final String id;
        public final String name;
        public final String category;

        Entry(String id, String name, String category) {
            this.id = id;
            this.name = name;
            this.category = category;
        }

        public String texture() {
            return "sky/" + id + ".png";
        }

        public String thumbnail() {
            return "sky/thumbs/" + id + ".png";
        }
    }

    private final List<Entry> entries;

    private SkyCatalog(List<Entry> entries) {
        this.entries = Collections.unmodifiableList(entries);
    }

    public static SkyCatalog empty() {
        return new SkyCatalog(new ArrayList<Entry>());
    }

    public List<Entry> entries() {
        return entries;
    }

    public Entry get(String id) {
        for (Entry e : entries) {
            if (e.id.equals(id)) return e;
        }
        return null;
    }

    /** Human-readable category name. */
    public static String categoryLabel(String category) {
        if ("atmospheric".equals(category)) return "Clouds & Aurora";
        if ("sunsets".equals(category)) return "Sunsets";
        if ("blocky".equals(category)) return "Blocky";
        return Character.toUpperCase(category.charAt(0)) + category.substring(1);
    }

    /** Parse; malformed entries throw so a broken build is caught by tests. */
    public static SkyCatalog parse(String json) {
        JsonElement root = new JsonParser().parse(json);
        if (!root.isJsonObject()) throw new IllegalArgumentException("sky catalog must be a JSON object");
        JsonElement skies = root.getAsJsonObject().get("skies");
        if (skies == null || !skies.isJsonArray()) throw new IllegalArgumentException("sky catalog has no skies array");
        List<Entry> out = new ArrayList<Entry>();
        Set<String> seen = new HashSet<String>();
        for (JsonElement el : skies.getAsJsonArray()) {
            if (!el.isJsonObject()) throw new IllegalArgumentException("sky entry must be an object");
            JsonObject o = el.getAsJsonObject();
            String id = str(o, "id");
            if (!id.matches("[a-z0-9_]{1,32}")) throw new IllegalArgumentException("invalid sky id: " + id);
            if (!seen.add(id)) throw new IllegalArgumentException("duplicate sky id: " + id);
            String category = str(o, "category");
            if (!Arrays.asList(CATEGORIES).contains(category)) {
                throw new IllegalArgumentException(id + ": unknown category " + category);
            }
            out.add(new Entry(id, str(o, "name"), category));
        }
        return new SkyCatalog(out);
    }

    private static String str(JsonObject o, String k) {
        JsonElement e = o.get(k);
        if (e == null || !e.isJsonPrimitive() || !e.getAsJsonPrimitive().isString() || e.getAsString().isEmpty()) {
            throw new IllegalArgumentException("missing string field " + k);
        }
        return e.getAsString();
    }
}
