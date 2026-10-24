package com.kestrel.effects;

/** Available kill effects. Ids are stored in the config. */
public enum KillEffectType {
    BURST("burst", "Particle burst"),
    RING("ring", "Particle ring"),
    HELIX("helix", "Colour helix"),
    DUST("dust", "Colour burst"),
    EXPLOSION("explosion", "Explosion"),
    LIGHTNING("lightning", "Lightning"),
    HEARTS("hearts", "Hearts"),
    FLAMES("flames", "Flame pillar"),
    ARCANE("arcane", "Arcane");

    public final String id;
    public final String label;

    KillEffectType(String id, String label) {
        this.id = id;
        this.label = label;
    }

    public static KillEffectType byId(String id) {
        for (KillEffectType t : values()) {
            if (t.id.equals(id)) return t;
        }
        return BURST;
    }
}
