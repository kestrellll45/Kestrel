package com.kestrel.skin;

import com.mojang.authlib.GameProfile;
import net.minecraft.util.ResourceLocation;

/** Static entry points for the skin mixins. */
public final class SkinHooks {
    private static SkinManager manager;

    private SkinHooks() {
    }

    static void setManager(SkinManager m) {
        manager = m;
    }

    /** Custom skin texture for this profile, or null to keep vanilla's. */
    public static ResourceLocation skin(GameProfile profile) {
        SkinManager m = manager;
        return m == null ? null : m.skinFor(profile);
    }

    /** "default"/"slim" for this profile, or null to keep vanilla's. */
    public static String model(GameProfile profile) {
        SkinManager m = manager;
        return m == null ? null : m.modelFor(profile);
    }
}
