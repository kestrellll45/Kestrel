package com.kestrel.cosmetics;

import java.awt.Graphics2D;
import java.awt.RenderingHints;
import java.awt.image.BufferedImage;

/**
 * Turns source images into Minecraft cape textures.
 *
 * <p>The vanilla cape model is a 10x16x1 box textured from a 64x32 sheet
 * (scaled by an integer factor for HD capes): the outer face (seen from
 * behind the player) is at (1,1), the inner face (facing the player's back) at
 * (12,1), with 1-texel edges around them. {@link Mode#FACES} fits an
 * arbitrary picture onto both large faces, unmirrored, so it reads correctly
 * from either side; {@link Mode#CAPE} takes an image that already uses the
 * cape layout. Pure Java (no Minecraft types) so it is unit-tested.
 */
public final class CapeLayout {
    public enum Mode { CAPE, FACES }

    /** Texture width for scale 1. Height is always width / 2. */
    static final int BASE_W = 64;
    static final int MAX_SCALE = 16;

    public static final class Spec {
        public final Mode mode;
        /** Texels per vanilla texel (texture is 64*scale x 32*scale). */
        public final int scale;
        /** Optional source crop {x, y, w, h}, or null for the whole image. */
        public final int[] crop;
        /** Opaque ARGB colour behind the picture on each face. */
        public final int fill;
        /** Opaque ARGB colour of the 1-texel edges. */
        public final int edge;

        public Spec(Mode mode, int scale, int[] crop, int fill, int edge) {
            if (scale < 1 || scale > MAX_SCALE) throw new IllegalArgumentException("scale must be 1.." + MAX_SCALE);
            if (crop != null && (crop.length != 4 || crop[2] <= 0 || crop[3] <= 0 || crop[0] < 0 || crop[1] < 0)) {
                throw new IllegalArgumentException("crop must be {x, y, w, h} with a positive size");
            }
            this.mode = mode;
            this.scale = scale;
            this.crop = crop == null ? null : crop.clone();
            this.fill = fill | 0xFF000000;
            this.edge = edge | 0xFF000000;
        }

        public int textureWidth() {
            return BASE_W * scale;
        }

        public int textureHeight() {
            return BASE_W * scale / 2;
        }
    }

    private CapeLayout() {
    }

    /** Whether an image already uses the cape sheet layout (2:1, width a multiple of 64). */
    public static boolean isCapeSheet(int width, int height) {
        return width >= BASE_W && width % BASE_W == 0 && width == height * 2 && width <= BASE_W * MAX_SCALE;
    }

    /** Build one cape texture frame. Never returns {@code src} itself. */
    public static BufferedImage build(BufferedImage src, Spec spec) {
        if (spec.mode == Mode.CAPE) {
            if (!isCapeSheet(src.getWidth(), src.getHeight())) {
                throw new IllegalArgumentException("not a cape sheet: " + src.getWidth() + "x" + src.getHeight());
            }
            BufferedImage out = new BufferedImage(src.getWidth(), src.getHeight(), BufferedImage.TYPE_INT_ARGB);
            Graphics2D g = out.createGraphics();
            try {
                g.drawImage(src, 0, 0, null);
            } finally {
                g.dispose();
            }
            return out;
        }

        int s = spec.scale;
        BufferedImage out = new BufferedImage(spec.textureWidth(), spec.textureHeight(), BufferedImage.TYPE_INT_ARGB);
        // Edges: top (1..11, 0), bottom (11..21, 0), left (0, 1..17), right (11, 1..17).
        fillRect(out, s, 0, 10 * s, s, spec.edge);
        fillRect(out, 11 * s, 0, 10 * s, s, spec.edge);
        fillRect(out, 0, s, s, 16 * s, spec.edge);
        fillRect(out, 11 * s, s, s, 16 * s, spec.edge);

        BufferedImage picture = fit(crop(src, spec.crop), 10 * s, 16 * s, spec.fill);
        Graphics2D g = out.createGraphics();
        try {
            g.drawImage(picture, s, s, null);       // outer face
            g.drawImage(picture, 12 * s, s, null);  // inner face
        } finally {
            g.dispose();
        }
        return out;
    }

    static BufferedImage crop(BufferedImage src, int[] crop) {
        if (crop == null) return src;
        int x = Math.min(crop[0], src.getWidth() - 1);
        int y = Math.min(crop[1], src.getHeight() - 1);
        int w = Math.min(crop[2], src.getWidth() - x);
        int h = Math.min(crop[3], src.getHeight() - y);
        return src.getSubimage(x, y, w, h);
    }

    /** Scale to fit inside w x h (aspect kept), centred on an opaque fill colour. */
    static BufferedImage fit(BufferedImage src, int w, int h, int fill) {
        double k = Math.min((double) w / src.getWidth(), (double) h / src.getHeight());
        int dw = Math.max(1, (int) Math.round(src.getWidth() * k));
        int dh = Math.max(1, (int) Math.round(src.getHeight() * k));
        BufferedImage scaled = resize(src, dw, dh);
        BufferedImage out = new BufferedImage(w, h, BufferedImage.TYPE_INT_ARGB);
        fillRect(out, 0, 0, w, h, fill);
        Graphics2D g = out.createGraphics();
        try {
            g.drawImage(scaled, (w - dw) / 2, (h - dh) / 2, null);
        } finally {
            g.dispose();
        }
        return out;
    }

    /**
     * Bilinear resize with repeated halving when shrinking by more than 2x,
     * so large downscales are not aliased.
     */
    static BufferedImage resize(BufferedImage src, int w, int h) {
        BufferedImage cur = src;
        int cw = src.getWidth();
        int ch = src.getHeight();
        do {
            int nw = cw / 2 >= w ? cw / 2 : w;
            int nh = ch / 2 >= h ? ch / 2 : h;
            BufferedImage next = new BufferedImage(nw, nh, BufferedImage.TYPE_INT_ARGB);
            Graphics2D g = next.createGraphics();
            try {
                g.setRenderingHint(RenderingHints.KEY_INTERPOLATION, RenderingHints.VALUE_INTERPOLATION_BILINEAR);
                g.setRenderingHint(RenderingHints.KEY_RENDERING, RenderingHints.VALUE_RENDER_QUALITY);
                g.drawImage(cur, 0, 0, nw, nh, null);
            } finally {
                g.dispose();
            }
            cur = next;
            cw = nw;
            ch = nh;
        } while (cw != w || ch != h);
        return cur;
    }

    private static void fillRect(BufferedImage img, int x, int y, int w, int h, int argb) {
        for (int yy = y; yy < y + h; yy++) {
            for (int xx = x; xx < x + w; xx++) img.setRGB(xx, yy, argb);
        }
    }
}
