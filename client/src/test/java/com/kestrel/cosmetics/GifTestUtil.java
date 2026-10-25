package com.kestrel.cosmetics;

import java.io.ByteArrayOutputStream;

/**
 * Writes small GIF89a files byte by byte, with explicit frame placement,
 * disposal, delay and transparency, so decoder tests do not depend on an
 * encoder's choices. Pixel data uses "uncompressed" LZW (literal 9-bit codes
 * with a clear code every 200 pixels), which every decoder must accept.
 */
final class GifTestUtil {
    static final int TRANSPARENT = 0;
    /** Palette: 0 transparent, 1 red, 2 green, 3 blue. */
    static final int RED = 0xFFFF0000, GREEN = 0xFF00FF00, BLUE = 0xFF0000FF;

    static final class Frame {
        final int x, y, w, h, colorIndex, delayCs;
        final String disposal;
        final boolean transparentHole;

        Frame(int x, int y, int w, int h, int colorIndex, int delayCs, String disposal, boolean transparentHole) {
            this.x = x; this.y = y; this.w = w; this.h = h;
            this.colorIndex = colorIndex; this.delayCs = delayCs;
            this.disposal = disposal; this.transparentHole = transparentHole;
        }
    }

    private GifTestUtil() {
    }

    static byte[] gif(int screenW, int screenH, Frame... frames) {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        writeAscii(out, "GIF89a");
        le16(out, screenW);
        le16(out, screenH);
        out.write(0x80 | 0x70 | 0x07); // global colour table, 256 entries
        out.write(0);                  // background index
        out.write(0);                  // aspect
        for (int i = 0; i < 256; i++) {
            out.write(i == 1 ? 255 : 0);
            out.write(i == 2 ? 255 : 0);
            out.write(i == 3 ? 255 : 0);
        }
        for (Frame f : frames) {
            // Graphic control extension.
            out.write(0x21);
            out.write(0xF9);
            out.write(4);
            out.write((disposalCode(f.disposal) << 2) | 1); // transparent colour flag
            le16(out, f.delayCs);
            out.write(TRANSPARENT);
            out.write(0);
            // Image descriptor.
            out.write(0x2C);
            le16(out, f.x);
            le16(out, f.y);
            le16(out, f.w);
            le16(out, f.h);
            out.write(0);
            out.write(8); // LZW minimum code size
            int[] pixels = new int[f.w * f.h];
            for (int yy = 0; yy < f.h; yy++) {
                for (int xx = 0; xx < f.w; xx++) {
                    boolean hole = f.transparentHole && xx == f.w / 2 && yy == f.h / 2;
                    pixels[yy * f.w + xx] = hole ? TRANSPARENT : f.colorIndex;
                }
            }
            writeSubBlocks(out, lzwLiterals(pixels));
        }
        out.write(0x3B);
        return out.toByteArray();
    }

    private static int disposalCode(String s) {
        if ("doNotDispose".equals(s)) return 1;
        if ("restoreToBackgroundColor".equals(s)) return 2;
        if ("restoreToPrevious".equals(s)) return 3;
        return 0;
    }

    private static byte[] lzwLiterals(int[] pixels) {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        long acc = 0;
        int bits = 0;
        int sinceClear = 0;
        int[] codes = new int[pixels.length + pixels.length / 200 + 3];
        int n = 0;
        codes[n++] = 256; // clear
        for (int p : pixels) {
            if (sinceClear == 200) {
                codes[n++] = 256;
                sinceClear = 0;
            }
            codes[n++] = p;
            sinceClear++;
        }
        codes[n++] = 257; // end of information
        for (int i = 0; i < n; i++) {
            acc |= ((long) codes[i]) << bits;
            bits += 9;
            while (bits >= 8) {
                bytes.write((int) (acc & 0xFF));
                acc >>>= 8;
                bits -= 8;
            }
        }
        if (bits > 0) bytes.write((int) (acc & 0xFF));
        return bytes.toByteArray();
    }

    private static void writeSubBlocks(ByteArrayOutputStream out, byte[] data) {
        for (int off = 0; off < data.length; off += 255) {
            int len = Math.min(255, data.length - off);
            out.write(len);
            out.write(data, off, len);
        }
        out.write(0);
    }

    private static void le16(ByteArrayOutputStream out, int v) {
        out.write(v & 0xFF);
        out.write((v >> 8) & 0xFF);
    }

    private static void writeAscii(ByteArrayOutputStream out, String s) {
        for (int i = 0; i < s.length(); i++) out.write(s.charAt(i));
    }
}
