package com.kestrel.util;

import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;

/**
 * Validates user-supplied PNG files before any image decoder touches them:
 * size limit, PNG signature, and dimensions read directly from the IHDR
 * chunk. Pure Java (no Minecraft types) so it is unit-tested.
 */
public final class ImageValidation {
    private static final byte[] PNG_SIGNATURE = {(byte) 0x89, 'P', 'N', 'G', '\r', '\n', 0x1A, '\n'};

    private ImageValidation() {
    }

    public static final class Dimensions {
        public final int width;
        public final int height;

        public Dimensions(int width, int height) {
            this.width = width;
            this.height = height;
        }

        @Override
        public String toString() {
            return width + "x" + height;
        }
    }

    /** Thrown for any file that must not be loaded; message is user-facing. */
    public static final class InvalidImageException extends Exception {
        private static final long serialVersionUID = 1L;

        public InvalidImageException(String message) {
            super(message);
        }
    }

    /** Read and check the PNG header; does not decode pixel data. */
    public static Dimensions readPngHeader(File file, long maxBytes) throws InvalidImageException {
        if (!file.isFile()) {
            throw new InvalidImageException("not a file: " + file.getName());
        }
        long len = file.length();
        if (len > maxBytes) {
            throw new InvalidImageException(file.getName() + " is too large (" + (len / 1024) + " KB, max " + (maxBytes / 1024) + " KB)");
        }
        byte[] head = new byte[24];
        InputStream in = null;
        try {
            in = new FileInputStream(file);
            int read = 0;
            while (read < head.length) {
                int n = in.read(head, read, head.length - read);
                if (n < 0) break;
                read += n;
            }
            if (read < head.length) {
                throw new InvalidImageException(file.getName() + " is not a PNG image");
            }
        } catch (IOException e) {
            throw new InvalidImageException("cannot read " + file.getName() + ": " + e.getMessage());
        } finally {
            if (in != null) {
                try {
                    in.close();
                } catch (IOException ignored) {
                    // nothing useful to do
                }
            }
        }
        return parseHeader(head, file.getName());
    }

    static Dimensions parseHeader(byte[] head, String name) throws InvalidImageException {
        for (int i = 0; i < PNG_SIGNATURE.length; i++) {
            if (head[i] != PNG_SIGNATURE[i]) {
                throw new InvalidImageException(name + " is not a PNG image");
            }
        }
        // Bytes 12..15 must be the IHDR chunk type; width/height follow.
        if (head[12] != 'I' || head[13] != 'H' || head[14] != 'D' || head[15] != 'R') {
            throw new InvalidImageException(name + " has a corrupt PNG header");
        }
        int w = readInt(head, 16);
        int h = readInt(head, 20);
        if (w <= 0 || h <= 0) {
            throw new InvalidImageException(name + " has invalid dimensions");
        }
        return new Dimensions(w, h);
    }

    private static int readInt(byte[] b, int off) {
        return ((b[off] & 0xFF) << 24) | ((b[off + 1] & 0xFF) << 16) | ((b[off + 2] & 0xFF) << 8) | (b[off + 3] & 0xFF);
    }

    /**
     * Cape textures: 2:1 per frame (64x32 classic, or HD multiples up to
     * 1024x512). Animated capes stack frames vertically, so the height may be
     * any whole multiple of width/2, up to 64 frames and 2048 px in total.
     *
     * @return the number of animation frames (1 for a static cape)
     */
    public static int validateCape(Dimensions d) throws InvalidImageException {
        if (d.width < 64 || d.width > 1024 || d.width % 64 != 0) {
            throw new InvalidImageException("cape width must be a multiple of 64 between 64 and 1024 (got " + d + ")");
        }
        int frameH = d.width / 2;
        if (d.height % frameH != 0) {
            throw new InvalidImageException("cape height must be a multiple of width/2 (got " + d + ")");
        }
        // Bound decoded size: <= 2048 rows (8 MB of ARGB at 1024 wide).
        if (d.height > 2048) {
            throw new InvalidImageException("cape image is too tall (max 2048 px, got " + d + ")");
        }
        int frames = d.height / frameH;
        if (frames < 1 || frames > 64) {
            throw new InvalidImageException("animated capes may have at most 64 frames (got " + frames + ")");
        }
        return frames;
    }

    /** Sky atlases: 3x2 grid of square faces, width 384..3072 (<= 25 MB decoded). */
    public static void validateSky(Dimensions d) throws InvalidImageException {
        if (d.width * 2 != d.height * 3) {
            throw new InvalidImageException("sky image must be a 3x2 grid of square faces, i.e. 3:2 (got " + d + ")");
        }
        if (d.width < 384 || d.width > 3072) {
            throw new InvalidImageException("sky width must be between 384 and 3072 (got " + d + ")");
        }
    }
}
