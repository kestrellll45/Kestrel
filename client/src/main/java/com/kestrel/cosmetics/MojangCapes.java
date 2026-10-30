package com.kestrel.cosmetics;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * Historical Mojang capes offered as local Kestrel cosmetics
 * ({@code assets/kestrel/mojang_capes.json}).
 *
 * <p>Only names and Mojang texture ids ship with Kestrel. The original
 * textures are downloaded from Mojang's texture server on first use and
 * cached on disk (see {@link CapeManager}); nothing is drawn for other players
 * and no account cape is granted or changed. The launcher reads the same file.
 *
 * <p>Pure Java (no Minecraft types) so it is unit-tested.
 */
public final class MojangCapes {
    /** Selection id prefix in the config ({@code mojang:minecon_2011}). */
    public static final String PREFIX = "mojang:";
    /** Menu category of these capes. */
    public static final String CATEGORY = "mojang";
    /** The only host textures are fetched from. */
    public static final String HOST = "textures.minecraft.net";

    public static final class Entry {
        public final String id;
        public final String name;
        /** Lower-case hex (Mojang drops leading zeros, so 32-64 digits), as in textures.minecraft.net/texture/&lt;id&gt;. */
        public final String textureId;
        /** One line on who received it. */
        public final String about;

        Entry(String id, String name, String textureId, String about) {
            this.id = id;
            this.name = name;
            this.textureId = textureId;
            this.about = about;
        }

        /** The selection id stored in the config. */
        public String selectionId() {
            return PREFIX + id;
        }

        /** Cache file name. */
        public String fileName() {
            return textureId + ".png";
        }

        public String httpsUrl() {
            return "https://" + HOST + "/texture/" + textureId;
        }

        /** What vanilla 1.8.9 itself uses for capes; only a fallback. */
        public String httpUrl() {
            return "http://" + HOST + "/texture/" + textureId;
        }
    }

    private final List<Entry> entries;

    private MojangCapes(List<Entry> entries) {
        this.entries = Collections.unmodifiableList(entries);
    }

    public static MojangCapes empty() {
        return new MojangCapes(new ArrayList<Entry>());
    }

    public List<Entry> entries() {
        return entries;
    }

    /** Entry for a selection id ({@code mojang:<id>}), or null. */
    public Entry bySelection(String selection) {
        if (selection == null || !selection.startsWith(PREFIX)) return null;
        String id = selection.substring(PREFIX.length());
        for (Entry e : entries) {
            if (e.id.equals(id)) return e;
        }
        return null;
    }

    public static boolean isTextureId(String s) {
        return s != null && s.matches("[0-9a-f]{32,64}");
    }

    /** Parse; malformed entries throw so a broken build is caught by tests. */
    public static MojangCapes parse(String json) {
        JsonElement root = new JsonParser().parse(json);
        if (!root.isJsonObject()) throw new IllegalArgumentException("Mojang cape list must be a JSON object");
        JsonElement capes = root.getAsJsonObject().get("capes");
        if (capes == null || !capes.isJsonArray()) throw new IllegalArgumentException("Mojang cape list has no capes array");
        List<Entry> out = new ArrayList<Entry>();
        Set<String> ids = new HashSet<String>();
        Set<String> textures = new HashSet<String>();
        for (JsonElement el : capes.getAsJsonArray()) {
            if (!el.isJsonObject()) throw new IllegalArgumentException("cape entry must be an object");
            JsonObject o = el.getAsJsonObject();
            String id = str(o, "id");
            if (!id.matches("[a-z0-9_]{1,32}")) throw new IllegalArgumentException("invalid cape id: " + id);
            if (!ids.add(id)) throw new IllegalArgumentException("duplicate cape id: " + id);
            String textureId = str(o, "textureId");
            if (!isTextureId(textureId)) throw new IllegalArgumentException(id + ": textureId must be 32-64 lower-case hex digits");
            if (!textures.add(textureId)) throw new IllegalArgumentException(id + ": duplicate textureId");
            out.add(new Entry(id, str(o, "name"), textureId, o.has("about") ? str(o, "about") : ""));
        }
        return new MojangCapes(out);
    }

    private static String str(JsonObject o, String k) {
        JsonElement e = o.get(k);
        if (e == null || !e.isJsonPrimitive() || !e.getAsJsonPrimitive().isString() || e.getAsString().isEmpty()) {
            throw new IllegalArgumentException("missing string field " + k);
        }
        return e.getAsString();
    }
}
