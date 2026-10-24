package com.kestrel.gui;

/** Colours shared with the launcher's dark theme. */
public final class Theme {
    public static final int ACCENT = 0xFFE8833A;
    public static final int ACCENT_DIM = 0xFF8A4F24;
    public static final int BACKDROP = 0xA0000000;
    public static final int PANEL = 0xF01B1D21;
    public static final int SIDEBAR = 0xF0141518;
    public static final int CARD = 0xFF22252A;
    public static final int CARD_HOVER = 0xFF2B2F35;
    public static final int TEXT = 0xFFE6E8EB;
    public static final int MUTED = 0xFF9AA0A8;
    public static final int OK = 0xFF5BC27A;
    public static final int WARN = 0xFFE3B341;
    public static final int ERR = 0xFFE05A5A;
    public static final int TRACK = 0xFF3A3E45;

    /** Palette offered by colour pickers (ARGB, opaque). */
    public static final int[] PALETTE = {
            0xFFFFFFFF, 0xFFE6E8EB, 0xFFAAAAAA, 0xFF000000, 0xFFE8833A, 0xFFFF5555, 0xFFFFAA00,
            0xFFFFFF55, 0xFF55FF55, 0xFF00AA00, 0xFF55FFFF, 0xFF00AAAA, 0xFF5555FF, 0xFFAA00AA, 0xFFFF55FF,
    };

    private Theme() {
    }
}
