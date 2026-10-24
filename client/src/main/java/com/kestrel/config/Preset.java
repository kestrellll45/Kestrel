package com.kestrel.config;

/** Performance/appearance presets shared with the launcher (ids must match). */
public enum Preset {
    POTATO("potato", "Potato"),
    COMPETITIVE("competitive", "Competitive"),
    VISUAL("visual", "Visual");

    public final String id;
    public final String label;

    Preset(String id, String label) {
        this.id = id;
        this.label = label;
    }

    public static Preset byId(String id) {
        if (id == null) return null;
        for (Preset p : values()) {
            if (p.id.equalsIgnoreCase(id.trim())) return p;
        }
        return null;
    }
}
