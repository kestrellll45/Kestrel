package com.kestrel.cosmetics;

import org.junit.Test;

import java.awt.image.BufferedImage;
import java.io.IOException;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

import static com.kestrel.cosmetics.GifTestUtil.BLUE;
import static com.kestrel.cosmetics.GifTestUtil.GREEN;
import static com.kestrel.cosmetics.GifTestUtil.RED;
import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

public class GifDecoderTest {
    private static final class Collect implements GifDecoder.FrameSink {
        final List<BufferedImage> frames = new ArrayList<BufferedImage>();
        final List<Integer> delays = new ArrayList<Integer>();

        @Override
        public void frame(int index, BufferedImage canvas, int delayMs) {
            assertEquals(frames.size(), index);
            BufferedImage copy = new BufferedImage(canvas.getWidth(), canvas.getHeight(), BufferedImage.TYPE_INT_ARGB);
            copy.getGraphics().drawImage(canvas, 0, 0, null);
            frames.add(copy);
            delays.add(delayMs);
        }
    }

    private static Collect decode(byte[] gif) throws IOException {
        Collect c = new Collect();
        GifDecoder.decode(gif, GifDecoder.CAPE_LIMITS, c);
        return c;
    }

    private static GifTestUtil.Frame f(int x, int y, int w, int h, int color, int delay, String disposal) {
        return new GifTestUtil.Frame(x, y, w, h, color, delay, disposal, false);
    }

    @Test
    public void subRectanglesAreCompositedWithDoNotDispose() throws Exception {
        Collect c = decode(GifTestUtil.gif(4, 4,
                f(0, 0, 4, 4, 1, 5, "doNotDispose"),
                f(2, 2, 2, 2, 2, 7, "doNotDispose")));
        assertEquals(2, c.frames.size());
        assertEquals(RED, c.frames.get(1).getRGB(0, 0));   // kept from frame 0
        assertEquals(GREEN, c.frames.get(1).getRGB(3, 3)); // new rectangle
        assertEquals(Arrays.asList(50, 70), c.delays);
    }

    @Test
    public void restoreToBackgroundClearsOnlyThePreviousRectangle() throws Exception {
        Collect c = decode(GifTestUtil.gif(4, 4,
                f(0, 0, 4, 4, 1, 10, "doNotDispose"),
                f(0, 0, 2, 2, 2, 10, "restoreToBackgroundColor"),
                f(3, 3, 1, 1, 3, 10, "doNotDispose")));
        BufferedImage last = c.frames.get(2);
        assertEquals(0, last.getRGB(0, 0) >>> 24);  // cleared to transparent
        assertEquals(RED, last.getRGB(2, 2));        // untouched by the disposal
        assertEquals(BLUE, last.getRGB(3, 3));
    }

    @Test
    public void restoreToPreviousRestoresTheCanvas() throws Exception {
        Collect c = decode(GifTestUtil.gif(4, 4,
                f(0, 0, 4, 4, 1, 10, "doNotDispose"),
                f(0, 0, 4, 4, 2, 10, "restoreToPrevious"),
                f(3, 3, 1, 1, 3, 10, "doNotDispose")));
        assertEquals(GREEN, c.frames.get(1).getRGB(0, 0));
        assertEquals(RED, c.frames.get(2).getRGB(0, 0)); // green frame was undone
        assertEquals(BLUE, c.frames.get(2).getRGB(3, 3));
    }

    @Test
    public void transparentPixelsShowThePreviousFrame() throws Exception {
        Collect c = decode(GifTestUtil.gif(4, 4,
                f(0, 0, 4, 4, 1, 10, "doNotDispose"),
                new GifTestUtil.Frame(0, 0, 4, 4, 2, 10, "doNotDispose", true)));
        assertEquals(RED, c.frames.get(1).getRGB(2, 2));   // hole
        assertEquals(GREEN, c.frames.get(1).getRGB(0, 0));
        assertEquals(RED, c.frames.get(0).getRGB(0, 0));
    }

    @Test
    public void tinyDelaysUseTheBrowserDefault() throws Exception {
        Collect c = decode(GifTestUtil.gif(2, 2,
                f(0, 0, 2, 2, 1, 0, "none"),
                f(0, 0, 2, 2, 2, 1, "none")));
        assertEquals(Arrays.asList(GifDecoder.DEFAULT_DELAY_MS, GifDecoder.DEFAULT_DELAY_MS), c.delays);
    }

    @Test
    public void framesOutsideTheScreenAreClipped() throws Exception {
        Collect c = decode(GifTestUtil.gif(2, 2,
                f(1, 1, 3, 3, 3, 10, "none")));
        assertEquals(2, c.frames.get(0).getWidth());
        assertEquals(BLUE, c.frames.get(0).getRGB(1, 1));
    }

    @Test
    public void rejectsNonGifData() {
        expectInvalid("PNG".getBytes(), "not a GIF");
        expectInvalid(new byte[0], "not a GIF");
        expectInvalid(ImageValidationBytes.png(), "not a GIF");
    }

    @Test
    public void rejectsOversizedScreenBeforeDecoding() throws Exception {
        byte[] gif = GifTestUtil.gif(2, 2, f(0, 0, 2, 2, 1, 10, "none"));
        gif[6] = (byte) 0x01;
        gif[7] = (byte) 0x08; // 2049 px wide
        expectInvalid(gif, "too large");
    }

    @Test
    public void rejectsOversizedFile() throws Exception {
        byte[] gif = Arrays.copyOf(GifTestUtil.gif(2, 2, f(0, 0, 2, 2, 1, 10, "none")), 5 * 1024 * 1024);
        expectInvalid(gif, "too large");
    }

    @Test
    public void rejectsTooManyFrames() throws Exception {
        GifTestUtil.Frame[] frames = new GifTestUtil.Frame[5];
        for (int i = 0; i < frames.length; i++) frames[i] = f(0, 0, 1, 1, 1, 10, "none");
        byte[] gif = GifTestUtil.gif(1, 1, frames);
        try {
            GifDecoder.decode(gif, new GifDecoder.Limits(1 << 20, 64, 4), new Collect());
            fail("expected rejection");
        } catch (IOException e) {
            assertTrue(e.getMessage(), e.getMessage().contains("too many frames"));
        }
    }

    @Test
    public void truncatedAndGarbledDataFailsCleanly() throws Exception {
        byte[] gif = GifTestUtil.gif(8, 8, f(0, 0, 8, 8, 1, 10, "none"), f(0, 0, 8, 8, 2, 10, "none"));
        for (int cut = 13; cut < gif.length; cut += 3) {
            byte[] t = Arrays.copyOf(gif, cut);
            try {
                GifDecoder.decode(t, GifDecoder.CAPE_LIMITS, new Collect());
            } catch (GifDecoder.InvalidGifException expected) {
                // fine: a typed, user-facing error
            }
        }
        java.util.Random rnd = new java.util.Random(42);
        for (int n = 0; n < 200; n++) {
            byte[] t = gif.clone();
            for (int k = 0; k < 4; k++) t[13 + rnd.nextInt(t.length - 13)] = (byte) rnd.nextInt();
            try {
                GifDecoder.decode(t, GifDecoder.CAPE_LIMITS, new Collect());
            } catch (GifDecoder.InvalidGifException expected) {
                // fine
            }
        }
    }

    @Test
    public void bundledTetoGifDecodesWithItsOwnTiming() throws Exception {
        byte[] data = resource("/assets/kestrel/textures/capes/teto.gif");
        assertArrayEquals(new int[]{231, 498}, GifDecoder.readHeader(data));
        Collect c = decode(data);
        assertEquals(24, c.frames.size());
        assertEquals(Integer.valueOf(70), c.delays.get(0));
        assertEquals(Integer.valueOf(60), c.delays.get(1));
        // Frame 4 only redraws (36,106)-(205,461); the rest must be composited
        // from earlier frames, so the letterbox bar is still opaque black.
        int top = c.frames.get(4).getRGB(5, 5);
        assertEquals(0xFF, top >>> 24);
        assertEquals(0, top & 0xF0F0F0);
        int pink = c.frames.get(4).getRGB(5, 200);
        assertTrue(Integer.toHexString(pink), ((pink >> 16) & 0xFF) > 240 && (pink & 0xFF) > 130);
    }

    static byte[] resource(String path) throws IOException {
        InputStream in = GifDecoderTest.class.getResourceAsStream(path);
        if (in == null) throw new IOException("missing test resource " + path);
        try {
            java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
            byte[] buf = new byte[8192];
            int n;
            while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
            return out.toByteArray();
        } finally {
            in.close();
        }
    }

    private static void expectInvalid(byte[] data, String message) {
        try {
            GifDecoder.decode(data, GifDecoder.CAPE_LIMITS, new Collect());
            fail("expected rejection");
        } catch (GifDecoder.InvalidGifException e) {
            assertTrue(e.getMessage(), e.getMessage().contains(message));
        } catch (IOException e) {
            fail("untyped error: " + e);
        }
    }

    /** A tiny valid PNG, to check that other formats are not accepted. */
    static final class ImageValidationBytes {
        static byte[] png() {
            try {
                java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
                javax.imageio.ImageIO.write(new BufferedImage(2, 2, BufferedImage.TYPE_INT_ARGB), "png", out);
                return out.toByteArray();
            } catch (IOException e) {
                throw new AssertionError(e);
            }
        }
    }
}
