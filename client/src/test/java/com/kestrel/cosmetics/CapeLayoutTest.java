package com.kestrel.cosmetics;

import org.junit.Test;

import java.awt.image.BufferedImage;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class CapeLayoutTest {
    private static final int FILL = 0xFFFF8298;
    private static final int EDGE = 0xFFC9576D;

    /** Left half red, right half blue, so mirroring would be visible. */
    private static BufferedImage halves(int w, int h) {
        BufferedImage img = new BufferedImage(w, h, BufferedImage.TYPE_INT_ARGB);
        for (int y = 0; y < h; y++) {
            for (int x = 0; x < w; x++) img.setRGB(x, y, x < w / 2 ? 0xFFFF0000 : 0xFF0000FF);
        }
        return img;
    }

    @Test
    public void facesLayoutPutsThePictureUnmirroredOnBothFaces() {
        CapeLayout.Spec spec = new CapeLayout.Spec(CapeLayout.Mode.FACES, 4, null, FILL, EDGE);
        BufferedImage out = CapeLayout.build(halves(40, 64), spec); // exactly 10:16
        assertEquals(256, out.getWidth());
        assertEquals(128, out.getHeight());
        int s = 4;
        // Outer face (1,1)-(11,17) and inner face (12,1)-(22,17): red on the left, blue on the right.
        for (int face : new int[]{1, 12}) {
            assertEquals(0xFFFF0000, out.getRGB(face * s + 2, 8 * s));
            assertEquals(0xFF0000FF, out.getRGB((face + 10) * s - 3, 8 * s));
        }
        // Edges.
        assertEquals(EDGE, out.getRGB(5 * s, 0));       // top
        assertEquals(EDGE, out.getRGB(15 * s, 0));      // bottom
        assertEquals(EDGE, out.getRGB(0, 8 * s));       // left side
        assertEquals(EDGE, out.getRGB(11 * s, 8 * s));  // right side
        // Unused area of the sheet stays transparent.
        assertEquals(0, out.getRGB(40 * s, 20 * s) >>> 24);
    }

    @Test
    public void narrowPicturesArePaddedWithTheFillColour() {
        CapeLayout.Spec spec = new CapeLayout.Spec(CapeLayout.Mode.FACES, 8, null, FILL, EDGE);
        BufferedImage out = CapeLayout.build(halves(20, 64), spec); // narrower than 10:16
        int s = 8;
        assertEquals(FILL, out.getRGB(s, 8 * s));                  // left padding
        assertEquals(FILL, out.getRGB(11 * s - 1, 8 * s));         // right padding
        assertEquals(0xFFFF0000, out.getRGB(6 * s - 6, 8 * s));    // picture
    }

    @Test
    public void cropRemovesLetterboxBars() {
        BufferedImage src = new BufferedImage(10, 30, BufferedImage.TYPE_INT_ARGB);
        for (int y = 0; y < 30; y++) {
            for (int x = 0; x < 10; x++) src.setRGB(x, y, y < 7 || y >= 23 ? 0xFF000000 : 0xFF00FF00);
        }
        CapeLayout.Spec spec = new CapeLayout.Spec(CapeLayout.Mode.FACES, 1, new int[]{0, 7, 10, 16}, FILL, EDGE);
        BufferedImage out = CapeLayout.build(src, spec);
        for (int y = 1; y < 17; y++) {
            for (int x = 1; x < 11; x++) assertEquals(0xFF00FF00, out.getRGB(x, y));
        }
    }

    @Test
    public void capeSheetsPassThroughUnchanged() {
        BufferedImage src = halves(128, 64);
        CapeLayout.Spec spec = new CapeLayout.Spec(CapeLayout.Mode.CAPE, 1, null, 0, 0);
        BufferedImage out = CapeLayout.build(src, spec);
        assertEquals(128, out.getWidth());
        assertEquals(src.getRGB(3, 3), out.getRGB(3, 3));
        assertEquals(src.getRGB(100, 60), out.getRGB(100, 60));
    }

    @Test
    public void capeSheetDetection() {
        assertTrue(CapeLayout.isCapeSheet(64, 32));
        assertTrue(CapeLayout.isCapeSheet(1024, 512));
        assertFalse(CapeLayout.isCapeSheet(231, 498));
        assertFalse(CapeLayout.isCapeSheet(96, 48));
        assertFalse(CapeLayout.isCapeSheet(2048, 1024));
    }

    @Test(expected = IllegalArgumentException.class)
    public void rejectsBadScale() {
        new CapeLayout.Spec(CapeLayout.Mode.FACES, 0, null, 0, 0);
    }

    @Test
    public void outOfRangeCropIsClamped() {
        CapeLayout.Spec spec = new CapeLayout.Spec(CapeLayout.Mode.FACES, 1, new int[]{5, 5, 500, 500}, FILL, EDGE);
        BufferedImage out = CapeLayout.build(halves(10, 16), spec);
        assertEquals(64, out.getWidth());
    }
}
