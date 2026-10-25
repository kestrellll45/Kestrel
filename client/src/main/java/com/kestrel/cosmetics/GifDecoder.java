package com.kestrel.cosmetics;

import org.w3c.dom.NamedNodeMap;
import org.w3c.dom.Node;

import javax.imageio.ImageIO;
import javax.imageio.ImageReader;
import javax.imageio.metadata.IIOMetadata;
import javax.imageio.stream.ImageInputStream;
import java.awt.AlphaComposite;
import java.awt.Graphics2D;
import java.awt.image.BufferedImage;
import java.io.ByteArrayInputStream;
import java.io.IOException;
import java.util.Iterator;

/**
 * Decodes animated GIFs into fully composited frames.
 *
 * <p>The JDK's GIF reader is used only for LZW decoding of individual frames;
 * everything else (sub-rectangle placement, disposal methods, transparency,
 * timing) is done here, because the reader returns raw frame rectangles and
 * leaves compositing to the caller.
 *
 * <p>Untrusted input is bounded before any pixel data is decoded: file size,
 * logical screen size, every frame's descriptor size, and frame count. Frames
 * are streamed to a {@link FrameSink} so at most two canvases are alive at
 * once. Pure Java (no Minecraft types) so it is unit-tested.
 */
public final class GifDecoder {
    /** GIF delays of 0 or 1 hundredths are shown at this rate, as browsers do. */
    static final int DEFAULT_DELAY_MS = 100;
    private static final String IMAGE_FORMAT = "javax_imageio_gif_image_1.0";

    public static final class Limits {
        public final long maxBytes;
        public final int maxSide;
        public final int maxFrames;

        public Limits(long maxBytes, int maxSide, int maxFrames) {
            this.maxBytes = maxBytes;
            this.maxSide = maxSide;
            this.maxFrames = maxFrames;
        }
    }

    /** Default limits for cape GIFs: 4 MB, 1024 px per side, 128 frames. */
    public static final Limits CAPE_LIMITS = new Limits(4L * 1024 * 1024, 1024, 128);

    /** Receives each composited frame. The canvas is reused: copy what you need. */
    public interface FrameSink {
        void frame(int index, BufferedImage canvas, int delayMs) throws IOException;
    }

    /** Thrown for any GIF that must not be used; message is user-facing. */
    public static final class InvalidGifException extends IOException {
        private static final long serialVersionUID = 1L;

        public InvalidGifException(String message) {
            super(message);
        }

        public InvalidGifException(String message, Throwable cause) {
            super(message, cause);
        }
    }

    enum Disposal { NONE, KEEP, BACKGROUND, PREVIOUS }

    private GifDecoder() {
    }

    /** Logical screen size from the header, without decoding anything. */
    public static int[] readHeader(byte[] data) throws InvalidGifException {
        if (data == null || data.length < 13) throw new InvalidGifException("not a GIF image");
        if (data[0] != 'G' || data[1] != 'I' || data[2] != 'F' || data[3] != '8'
                || (data[4] != '7' && data[4] != '9') || data[5] != 'a') {
            throw new InvalidGifException("not a GIF image");
        }
        int w = (data[6] & 0xFF) | ((data[7] & 0xFF) << 8);
        int h = (data[8] & 0xFF) | ((data[9] & 0xFF) << 8);
        if (w <= 0 || h <= 0) throw new InvalidGifException("GIF has invalid dimensions");
        return new int[]{w, h};
    }

    /**
     * Decode and composite every frame.
     *
     * @return the number of frames delivered (at least 1)
     */
    public static int decode(byte[] data, Limits limits, FrameSink sink) throws IOException {
        if (data.length > limits.maxBytes) {
            throw new InvalidGifException("GIF is too large (" + (data.length / 1024) + " KB, max "
                    + (limits.maxBytes / 1024) + " KB)");
        }
        int[] size = readHeader(data);
        int w = size[0];
        int h = size[1];
        if (w > limits.maxSide || h > limits.maxSide) {
            throw new InvalidGifException("GIF is too large (" + w + "x" + h + ", max "
                    + limits.maxSide + " px per side)");
        }

        ImageInputStream in = ImageIO.createImageInputStream(new ByteArrayInputStream(data));
        if (in == null) throw new IOException("cannot open GIF stream");
        ImageReader reader = null;
        try {
            Iterator<ImageReader> readers = ImageIO.getImageReadersByFormatName("gif");
            if (!readers.hasNext()) throw new IOException("no GIF decoder available");
            reader = readers.next();
            reader.setInput(in, true, false);
            return composite(reader, w, h, limits, sink);
        } catch (InvalidGifException e) {
            throw e;
        } catch (IOException e) {
            throw new InvalidGifException("corrupt GIF: " + e.getMessage(), e);
        } catch (RuntimeException e) {
            // The JDK reader can throw unchecked exceptions on malformed data.
            throw new InvalidGifException("corrupt GIF: " + e, e);
        } finally {
            if (reader != null) reader.dispose();
            in.close();
        }
    }

    private static int composite(ImageReader reader, int w, int h, Limits limits, FrameSink sink) throws IOException {
        BufferedImage canvas = new BufferedImage(w, h, BufferedImage.TYPE_INT_ARGB);
        BufferedImage saved = null;
        Disposal prevDisposal = Disposal.NONE;
        int px = 0, py = 0, pw = 0, ph = 0;
        int count = 0;
        for (int i = 0; ; i++) {
            IIOMetadata meta;
            try {
                meta = reader.getImageMetadata(i);
            } catch (IndexOutOfBoundsException end) {
                break;
            }
            if (i >= limits.maxFrames) {
                throw new InvalidGifException("GIF has too many frames (max " + limits.maxFrames + ")");
            }
            Node root = meta.getAsTree(IMAGE_FORMAT);
            Node desc = child(root, "ImageDescriptor");
            if (desc == null) throw new InvalidGifException("corrupt GIF: frame " + i + " has no descriptor");
            int fx = intAttr(desc, "imageLeftPosition", 0);
            int fy = intAttr(desc, "imageTopPosition", 0);
            int fw = intAttr(desc, "imageWidth", 0);
            int fh = intAttr(desc, "imageHeight", 0);
            // Bound the raster the reader will allocate before decoding it.
            if (fw <= 0 || fh <= 0 || fw > limits.maxSide || fh > limits.maxSide) {
                throw new InvalidGifException("corrupt GIF: frame " + i + " is " + fw + "x" + fh);
            }
            Node gce = child(root, "GraphicControlExtension");
            Disposal disposal = gce == null ? Disposal.NONE : parseDisposal(attr(gce, "disposalMethod"));
            int delayCs = gce == null ? 0 : intAttr(gce, "delayTime", 0);

            // Dispose of the previous frame.
            if (prevDisposal == Disposal.BACKGROUND) {
                clear(canvas, px, py, pw, ph);
            } else if (prevDisposal == Disposal.PREVIOUS && saved != null) {
                copyInto(saved, canvas);
            }
            if (disposal == Disposal.PREVIOUS) {
                if (saved == null) saved = new BufferedImage(w, h, BufferedImage.TYPE_INT_ARGB);
                copyInto(canvas, saved);
            }

            BufferedImage frame = reader.read(i);
            Graphics2D g = canvas.createGraphics();
            try {
                // Source-over honours the transparent colour index; pixels
                // outside the logical screen are clipped.
                g.setComposite(AlphaComposite.SrcOver);
                g.setClip(0, 0, w, h);
                g.drawImage(frame, fx, fy, null);
            } finally {
                g.dispose();
            }
            int delayMs = delayCs <= 1 ? DEFAULT_DELAY_MS : delayCs * 10;
            sink.frame(count, canvas, delayMs);
            count++;

            prevDisposal = disposal;
            px = fx;
            py = fy;
            pw = fw;
            ph = fh;
        }
        if (count == 0) throw new InvalidGifException("GIF has no frames");
        return count;
    }

    static Disposal parseDisposal(String s) {
        if ("doNotDispose".equals(s)) return Disposal.KEEP;
        if ("restoreToBackgroundColor".equals(s)) return Disposal.BACKGROUND;
        if ("restoreToPrevious".equals(s)) return Disposal.PREVIOUS;
        return Disposal.NONE; // "none" and undefined values behave like doNotDispose
    }

    /** Restore-to-background clears to transparent (what browsers do). */
    private static void clear(BufferedImage canvas, int x, int y, int w, int h) {
        int x0 = Math.max(0, x);
        int y0 = Math.max(0, y);
        int x1 = Math.min(canvas.getWidth(), x + w);
        int y1 = Math.min(canvas.getHeight(), y + h);
        for (int yy = y0; yy < y1; yy++) {
            for (int xx = x0; xx < x1; xx++) canvas.setRGB(xx, yy, 0);
        }
    }

    private static void copyInto(BufferedImage src, BufferedImage dst) {
        int[] row = new int[src.getWidth()];
        for (int y = 0; y < src.getHeight(); y++) {
            src.getRGB(0, y, row.length, 1, row, 0, row.length);
            dst.setRGB(0, y, row.length, 1, row, 0, row.length);
        }
    }

    private static Node child(Node n, String name) {
        for (Node c = n.getFirstChild(); c != null; c = c.getNextSibling()) {
            if (name.equals(c.getNodeName())) return c;
        }
        return null;
    }

    private static String attr(Node n, String name) {
        NamedNodeMap a = n.getAttributes();
        if (a == null) return null;
        Node v = a.getNamedItem(name);
        return v == null ? null : v.getNodeValue();
    }

    private static int intAttr(Node n, String name, int def) {
        String v = attr(n, name);
        if (v == null) return def;
        try {
            return Integer.parseInt(v);
        } catch (NumberFormatException e) {
            return def;
        }
    }
}
