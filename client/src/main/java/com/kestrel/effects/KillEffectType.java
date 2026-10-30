package com.kestrel.effects;

/**
 * Available kill effects. Ids are stored in the config and must match the
 * launcher's list ({@code KILL_EFFECTS} in kestrel-core's cosmetics.rs).
 */
public enum KillEffectType {
    // id, label, category, sound, pitch, uses the colour setting
    BURST("burst", "Particle burst", Category.CLASSIC, "random.orb", 0.6f, false),
    RING("ring", "Particle ring", Category.CLASSIC, "random.orb", 0.6f, false),
    HELIX("helix", "Colour helix", Category.CLASSIC, "random.orb", 0.6f, true),
    DUST("dust", "Colour burst", Category.CLASSIC, "random.orb", 0.6f, true),
    EXPLOSION("explosion", "Explosion", Category.CLASSIC, "random.explode", 1.2f, false),
    SMOKE("smoke", "Smoke bomb", Category.CLASSIC, "random.fizz", 0.6f, false),
    PIXEL("pixel", "Pixel blast", Category.CLASSIC, "random.pop", 0.8f, false),
    LIGHTNING("lightning", "Lightning", Category.ELEMENTAL, "ambient.weather.thunder", 1.4f, false),
    FLAMES("flames", "Flame pillar", Category.ELEMENTAL, "mob.ghast.fireball", 1.0f, false),
    SHOCK("shock", "Electric shock", Category.ELEMENTAL, "random.fizz", 1.8f, false),
    ICE("ice", "Ice shatter", Category.ELEMENTAL, "dig.glass", 1.2f, false),
    NETHER("nether", "Nether blaze", Category.ELEMENTAL, "mob.blaze.hit", 0.8f, false),
    TORNADO("tornado", "Tornado", Category.ELEMENTAL, "mob.enderdragon.wings", 1.2f, false),
    ARCANE("arcane", "Arcane", Category.MAGIC, "mob.endermen.portal", 1.2f, false),
    SOUL("soul", "Soul release", Category.MAGIC, "mob.ghast.moan", 1.6f, false),
    ENDER("ender", "Ender vanish", Category.MAGIC, "mob.endermen.portal", 0.8f, false),
    PORTAL("portal", "Portal", Category.MAGIC, "mob.endermen.portal", 0.6f, false),
    DRAGON("dragon", "Dragon breath", Category.MAGIC, "mob.enderdragon.growl", 1.6f, false),
    ENERGY("energy", "Energy burst", Category.MAGIC, "random.orb", 0.5f, true),
    HEARTS("hearts", "Hearts", Category.CELEBRATION, "random.levelup", 1.6f, false),
    FIREWORKS("fireworks", "Fireworks", Category.CELEBRATION, "fireworks.blast", 1.0f, true),
    CONFETTI("confetti", "Confetti", Category.CELEBRATION, "fireworks.twinkle", 1.2f, false),
    RAINBOW("rainbow", "Rainbow rings", Category.CELEBRATION, "random.orb", 1.4f, false),
    STARFALL("starfall", "Falling stars", Category.CELEBRATION, "fireworks.twinkle", 1.6f, false),
    KESTREL("kestrel", "Kestrel strike", Category.KESTREL, "mob.bat.takeoff", 0.7f, false);

    public enum Category {
        CLASSIC("Classic"), ELEMENTAL("Elemental"), MAGIC("Magic"), CELEBRATION("Celebration"), KESTREL("Kestrel");

        public final String label;

        Category(String label) {
            this.label = label;
        }
    }

    public final String id;
    public final String label;
    public final Category category;
    public final String sound;
    public final float pitch;
    /** Whether the effect is tinted by the colour setting. */
    public final boolean usesColor;

    KillEffectType(String id, String label, Category category, String sound, float pitch, boolean usesColor) {
        this.id = id;
        this.label = label;
        this.category = category;
        this.sound = sound;
        this.pitch = pitch;
        this.usesColor = usesColor;
    }

    public static KillEffectType byId(String id) {
        for (KillEffectType t : values()) {
            if (t.id.equals(id)) return t;
        }
        return BURST;
    }
}
