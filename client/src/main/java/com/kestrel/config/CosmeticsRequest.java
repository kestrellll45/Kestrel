package com.kestrel.config;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;

/**
 * Applies a cosmetics request written by the launcher
 * ({@code <gameDir>/kestrel/launcher-cosmetics.json}) to the client config.
 *
 * <p>The launcher never writes config.json (the client owns it). Instead it
 * writes a request with an increasing {@code revision}; each revision is
 * applied exactly once and recorded in
 * {@link ClientConfig#lastLauncherCosmeticsRevision}. Only known fields of
 * the right type are copied; everything is validated afterwards. Pure (no
 * Minecraft types) so it is unit-tested.
 */
public final class CosmeticsRequest {
    private CosmeticsRequest() {
    }

    /**
     * @return true if the request was new and has been applied
     * @throws RuntimeException (Gson) if the text is not valid JSON
     */
    public static boolean apply(ClientConfig c, String json) {
        JsonElement root = new JsonParser().parse(json);
        if (!root.isJsonObject()) return false;
        JsonObject o = root.getAsJsonObject();
        long revision = num(o, "revision") != null ? num(o, "revision").getAsLong() : 0L;
        if (revision <= c.lastLauncherCosmeticsRevision) return false;

        JsonObject cos = obj(o, "cosmetics");
        if (cos != null) {
            c.cosmetics.capesEnabled = bool(cos, "capesEnabled", c.cosmetics.capesEnabled);
            c.cosmetics.cape = str(cos, "cape", c.cosmetics.cape);
            c.cosmetics.animatedCapes = bool(cos, "animatedCapes", c.cosmetics.animatedCapes);
        }
        JsonObject sky = obj(o, "sky");
        if (sky != null) {
            c.sky.enabled = bool(sky, "enabled", c.sky.enabled);
            c.sky.preset = str(sky, "preset", c.sky.preset);
            c.sky.brightness = flt(sky, "brightness", c.sky.brightness);
            c.sky.rotate = bool(sky, "rotate", c.sky.rotate);
        }
        JsonObject ke = obj(o, "killEffects");
        if (ke != null) {
            c.killEffects.enabled = bool(ke, "enabled", c.killEffects.enabled);
            c.killEffects.effect = str(ke, "effect", c.killEffects.effect);
            c.killEffects.intensity = flt(ke, "intensity", c.killEffects.intensity);
            c.killEffects.sounds = bool(ke, "sounds", c.killEffects.sounds);
            JsonPrimitive col = num(ke, "color");
            if (col != null) {
                // The launcher writes an unsigned 32-bit ARGB value.
                c.killEffects.color = (int) col.getAsLong();
            }
        }
        c.lastLauncherCosmeticsRevision = revision;
        c.validate();
        return true;
    }

    private static JsonObject obj(JsonObject o, String k) {
        JsonElement e = o.get(k);
        return e != null && e.isJsonObject() ? e.getAsJsonObject() : null;
    }

    private static JsonPrimitive num(JsonObject o, String k) {
        JsonElement e = o.get(k);
        return e != null && e.isJsonPrimitive() && e.getAsJsonPrimitive().isNumber() ? e.getAsJsonPrimitive() : null;
    }

    private static boolean bool(JsonObject o, String k, boolean def) {
        JsonElement e = o.get(k);
        return e != null && e.isJsonPrimitive() && e.getAsJsonPrimitive().isBoolean() ? e.getAsBoolean() : def;
    }

    private static String str(JsonObject o, String k, String def) {
        JsonElement e = o.get(k);
        return e != null && e.isJsonPrimitive() && e.getAsJsonPrimitive().isString() ? e.getAsString() : def;
    }

    private static float flt(JsonObject o, String k, float def) {
        JsonPrimitive p = num(o, k);
        return p != null ? p.getAsFloat() : def;
    }
}
