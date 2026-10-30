package com.kestrel.skin;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import java.util.Locale;

/**
 * "Load from URL" for the in-game Skin screen; the same rules as the
 * launcher ({@code kestrel_core::skins::parse_source}).
 *
 * <p>Supported: NameMC profile links ({@code namemc.com/profile/<name|uuid>}),
 * resolved through Mojang's public profile API to the player's current skin,
 * and direct {@code textures.minecraft.net} texture links. NameMC skin pages
 * ({@code namemc.com/skin/<id>}) are recognised but cannot be fetched: NameMC
 * has no public API for them and blocks automated requests, so the user is
 * told to download the PNG and import it. Nothing is requested from any
 * other host, and no credentials or cookies are ever sent.
 */
public final class SkinUrl {
    static final int MAX_URL = 300;
    static final int MAX_JSON = 64 * 1024;
    static final int TIMEOUT_MS = 10_000;

    public static final String NAMEMC_SKIN_PAGE_HELP = "NameMC skin pages (namemc.com/skin/...) can't be loaded "
            + "automatically: NameMC has no public API for them and blocks automated downloads. Open the page in "
            + "your browser, click Download, then import the saved PNG. Profile links (namemc.com/profile/<name>) work directly.";

    public enum Kind { PROFILE, TEXTURE, NAMEMC_SKIN }

    /** A parsed link. */
    public static final class Source {
        public final Kind kind;
        /** Name or 32-hex UUID (PROFILE), https URL (TEXTURE), page id (NAMEMC_SKIN). */
        public final String value;

        Source(Kind kind, String value) {
            this.kind = kind;
            this.value = value;
        }
    }

    /** A downloaded skin, not yet imported. */
    public static final class Downloaded {
        public final byte[] bytes;
        public final String name;
        /** "slim"/"classic" when known, else null. */
        public final String model;

        Downloaded(byte[] bytes, String name, String model) {
            this.bytes = bytes;
            this.name = name;
            this.model = model;
        }
    }

    /** Network access, replaceable in tests. */
    public interface Fetcher {
        byte[] fetch(String url, int limit) throws IOException;
    }

    private SkinUrl() {
    }

    /** Parse a pasted link; throws with a user-facing message. */
    public static Source parse(String input) throws SkinStore.SkinException {
        String in = input == null ? "" : input.trim();
        if (in.isEmpty()) throw new SkinStore.SkinException("paste a NameMC profile link, e.g. https://namemc.com/profile/Notch");
        if (in.length() > MAX_URL) throw new SkinStore.SkinException("that link is too long");
        String rest;
        if (in.startsWith("https://")) rest = in.substring(8);
        else if (in.startsWith("http://")) rest = in.substring(7);
        else if (in.contains("://")) throw new SkinStore.SkinException("only http(s) links are supported");
        else rest = in;
        int end = indexOfAny(rest, "/?#");
        String host = rest.substring(0, end).toLowerCase(Locale.ROOT);
        if (host.contains("@") || host.contains(":")) {
            throw new SkinStore.SkinException("unsupported link (user names and ports are not allowed)");
        }
        String path = rest.substring(end);
        int q = indexOfAny(path, "?#");
        path = path.substring(0, q);
        String[] raw = path.split("/");
        java.util.List<String> segs = new java.util.ArrayList<String>();
        for (String s : raw) if (!s.isEmpty()) segs.add(s);

        if (isNameMcHost(host)) {
            if (segs.size() >= 2 && segs.get(0).equals("profile")) {
                String id = segs.get(1);
                int dot = id.lastIndexOf('.');
                if (dot > 0 && dot < id.length() - 1 && allDigits(id.substring(dot + 1))) id = id.substring(0, dot);
                String uuid = normalizeUuid(id);
                if (uuid != null) return new Source(Kind.PROFILE, uuid);
                if (isValidPlayerName(id)) return new Source(Kind.PROFILE, id);
                throw new SkinStore.SkinException("'" + id + "' is not a valid Minecraft name or UUID");
            }
            if (segs.size() >= 2 && segs.get(0).equals("skin") && segs.get(1).length() <= 64 && isAlnum(segs.get(1))) {
                return new Source(Kind.NAMEMC_SKIN, segs.get(1));
            }
            throw new SkinStore.SkinException("unsupported NameMC link: use a profile link (namemc.com/profile/<name>)");
        }
        if (host.equals("textures.minecraft.net")) {
            if (segs.size() == 2 && segs.get(0).equals("texture") && segs.get(1).length() <= 64 && isHex(segs.get(1))) {
                return new Source(Kind.TEXTURE, "https://textures.minecraft.net/texture/" + segs.get(1));
            }
            throw new SkinStore.SkinException("unsupported textures.minecraft.net link");
        }
        throw new SkinStore.SkinException("unsupported site '" + host
                + "': paste a NameMC profile link (namemc.com/profile/<name>) or a textures.minecraft.net link");
    }

    /** Resolve a parsed link to skin bytes. */
    public static Downloaded resolve(Source src, Fetcher fetch) throws SkinStore.SkinException, IOException {
        switch (src.kind) {
            case NAMEMC_SKIN:
                throw new SkinStore.SkinException(NAMEMC_SKIN_PAGE_HELP);
            case TEXTURE: {
                byte[] b = fetch.fetch(src.value, (int) SkinStore.MAX_SKIN_BYTES);
                String hash = src.value.substring(src.value.lastIndexOf('/') + 1);
                return new Downloaded(b, "texture " + hash.substring(0, Math.min(8, hash.length())), null);
            }
            default: {
                String uuid = normalizeUuid(src.value);
                String name = null;
                if (uuid == null) {
                    byte[] body;
                    try {
                        body = fetch.fetch("https://api.mojang.com/users/profiles/minecraft/" + src.value, MAX_JSON);
                    } catch (IOException e) {
                        String m = String.valueOf(e.getMessage());
                        if (m.contains("404") || m.contains("204")) {
                            throw new SkinStore.SkinException("no Minecraft account is named '" + src.value + "'");
                        }
                        throw e;
                    }
                    JsonObject o = object(body);
                    uuid = o == null ? null : normalizeUuid(string(o, "id"));
                    if (uuid == null) throw new SkinStore.SkinException("no Minecraft account is named '" + src.value + "'");
                    name = string(o, "name");
                }
                byte[] profile = fetch.fetch("https://sessionserver.mojang.com/session/minecraft/profile/" + uuid, MAX_JSON);
                String[] t = parseProfileTextures(profile);
                byte[] skin = fetch.fetch(t[0], (int) SkinStore.MAX_SKIN_BYTES);
                String display = t[2] != null ? t[2] : (name != null ? name : src.value);
                return new Downloaded(skin, display, t[1]);
            }
        }
    }

    /** @return {https texture URL, model, player name or null} */
    static String[] parseProfileTextures(byte[] body) throws SkinStore.SkinException {
        JsonObject o = object(body);
        if (o == null) throw new SkinStore.SkinException("unexpected reply from Mojang");
        String value = null;
        JsonElement props = o.get("properties");
        if (props != null && props.isJsonArray()) {
            JsonArray arr = props.getAsJsonArray();
            for (JsonElement p : arr) {
                if (p.isJsonObject() && "textures".equals(string(p.getAsJsonObject(), "name"))) {
                    value = string(p.getAsJsonObject(), "value");
                }
            }
        }
        if (value == null) throw new SkinStore.SkinException("this player has no skin information");
        JsonObject t;
        try {
            t = object(Base64.getDecoder().decode(value.trim()));
        } catch (IllegalArgumentException e) {
            t = null;
        }
        JsonObject textures = t == null ? null : obj(t, "textures");
        if (textures == null) throw new SkinStore.SkinException("unexpected reply from Mojang");
        JsonObject skin = obj(textures, "SKIN");
        if (skin == null) {
            throw new SkinStore.SkinException("this player uses a default skin (Steve/Alex); there is no custom skin to load");
        }
        String url = string(skin, "url");
        JsonObject meta = obj(skin, "metadata");
        String model = SkinStore.normalizeModel(meta == null ? null : string(meta, "model"));
        Source s;
        try {
            s = parse(url);
        } catch (SkinStore.SkinException e) {
            s = null;
        }
        if (s == null || s.kind != Kind.TEXTURE) throw new SkinStore.SkinException("unexpected skin URL from Mojang");
        return new String[] {s.value, model, string(o, "name")};
    }

    /**
     * The real fetcher: only the three Mojang hosts, https (Mojang's texture
     * host also over plain http, exactly as the game itself downloads every
     * skin, if the https request fails), no redirects, no cookies or
     * credentials, 10 s timeouts and a size cap.
     */
    public static final Fetcher HTTP = new Fetcher() {
        @Override
        public byte[] fetch(String url, int limit) throws IOException {
            String host = url.startsWith("https://") ? url.substring(8).split("/", 2)[0] : "";
            if (!host.equals("api.mojang.com") && !host.equals("sessionserver.mojang.com") && !host.equals("textures.minecraft.net")) {
                throw new IOException("refusing to contact " + url);
            }
            try {
                return get(url, limit);
            } catch (IOException e) {
                if (!host.equals("textures.minecraft.net") || e.getMessage() != null && e.getMessage().startsWith("HTTP ")) throw e;
                return get("http://" + url.substring(8), limit);
            }
        }
    };

    private static byte[] get(String url, int limit) throws IOException {
        HttpURLConnection c = (HttpURLConnection) new URL(url).openConnection();
        c.setInstanceFollowRedirects(false);
        c.setUseCaches(false);
        c.setConnectTimeout(TIMEOUT_MS);
        c.setReadTimeout(TIMEOUT_MS);
        c.setRequestProperty("User-Agent", "Kestrel-Client");
        try {
            int code = c.getResponseCode();
            if (code != 200) throw new IOException("HTTP " + code + " from " + c.getURL().getHost());
            long len = c.getContentLengthLong();
            if (len > limit) throw new IOException("response too large");
            InputStream in = c.getInputStream();
            try {
                ByteArrayOutputStream out = new ByteArrayOutputStream();
                byte[] buf = new byte[8192];
                int n;
                while ((n = in.read(buf)) > 0) {
                    out.write(buf, 0, n);
                    if (out.size() > limit) throw new IOException("response too large");
                }
                return out.toByteArray();
            } finally {
                in.close();
            }
        } finally {
            c.disconnect();
        }
    }

    // ---- helpers -------------------------------------------------------

    static boolean isNameMcHost(String host) {
        if (host.equals("namemc.com") || host.equals("www.namemc.com")) return true;
        if (!host.endsWith(".namemc.com")) return false;
        String sub = host.substring(0, host.length() - ".namemc.com".length());
        // NameMC's language subdomains (de.namemc.com, ...).
        return sub.length() == 2 && sub.charAt(0) >= 'a' && sub.charAt(0) <= 'z' && sub.charAt(1) >= 'a' && sub.charAt(1) <= 'z';
    }

    public static boolean isValidPlayerName(String s) {
        if (s == null || s.length() < 3 || s.length() > 16) return false;
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (!(c == '_' || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9'))) return false;
        }
        return true;
    }

    /** 32 hex digits with or without the four dashes, lowercase without dashes; else null. */
    public static String normalizeUuid(String s) {
        if (s == null || (s.length() != 32 && s.length() != 36)) return null;
        String hex = s.replace("-", "");
        if (hex.length() != 32 || !isHex(hex)) return null;
        return hex.toLowerCase(Locale.ROOT);
    }

    private static int indexOfAny(String s, String chars) {
        for (int i = 0; i < s.length(); i++) if (chars.indexOf(s.charAt(i)) >= 0) return i;
        return s.length();
    }

    private static boolean isHex(String s) {
        if (s.isEmpty()) return false;
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F'))) return false;
        }
        return true;
    }

    private static boolean isAlnum(String s) {
        if (s.isEmpty()) return false;
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9'))) return false;
        }
        return true;
    }

    private static boolean allDigits(String s) {
        for (int i = 0; i < s.length(); i++) if (s.charAt(i) < '0' || s.charAt(i) > '9') return false;
        return !s.isEmpty();
    }

    private static JsonObject object(byte[] body) {
        try {
            JsonElement e = new JsonParser().parse(new String(body, StandardCharsets.UTF_8));
            return e != null && e.isJsonObject() ? e.getAsJsonObject() : null;
        } catch (RuntimeException e) {
            return null;
        }
    }

    private static JsonObject obj(JsonObject o, String k) {
        JsonElement e = o.get(k);
        return e != null && e.isJsonObject() ? e.getAsJsonObject() : null;
    }

    private static String string(JsonObject o, String k) {
        JsonElement e = o.get(k);
        return e != null && e.isJsonPrimitive() && e.getAsJsonPrimitive().isString() ? e.getAsString() : null;
    }
}
