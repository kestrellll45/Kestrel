package com.kestrel.util;

import org.junit.Rule;
import org.junit.Test;
import org.junit.rules.TemporaryFolder;

import java.io.File;
import java.io.FileOutputStream;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.fail;

public class ImageValidationTest {
    @Rule
    public TemporaryFolder tmp = new TemporaryFolder();

    static byte[] pngHeader(int w, int h) {
        byte[] b = new byte[24];
        byte[] sig = {(byte) 0x89, 'P', 'N', 'G', '\r', '\n', 0x1A, '\n'};
        System.arraycopy(sig, 0, b, 0, 8);
        b[11] = 13;
        b[12] = 'I';
        b[13] = 'H';
        b[14] = 'D';
        b[15] = 'R';
        b[16] = (byte) (w >>> 24);
        b[17] = (byte) (w >>> 16);
        b[18] = (byte) (w >>> 8);
        b[19] = (byte) w;
        b[20] = (byte) (h >>> 24);
        b[21] = (byte) (h >>> 16);
        b[22] = (byte) (h >>> 8);
        b[23] = (byte) h;
        return b;
    }

    private File file(byte[] data) throws Exception {
        File f = tmp.newFile();
        FileOutputStream o = new FileOutputStream(f);
        o.write(data);
        o.close();
        return f;
    }

    @Test
    public void readsDimensions() throws Exception {
        ImageValidation.Dimensions d = ImageValidation.readPngHeader(file(pngHeader(64, 32)), 1024);
        assertEquals(64, d.width);
        assertEquals(32, d.height);
    }

    @Test
    public void rejectsNonPngAndOversized() throws Exception {
        expectInvalid(file("GIF89a............................".getBytes("US-ASCII")), 1024);
        expectInvalid(file(new byte[3]), 1024);
        expectInvalid(file(pngHeader(64, 32)), 10);
    }

    @Test
    public void capeRules() throws Exception {
        assertEquals(1, ImageValidation.validateCape(new ImageValidation.Dimensions(64, 32)));
        assertEquals(8, ImageValidation.validateCape(new ImageValidation.Dimensions(64, 256)));
        assertEquals(1, ImageValidation.validateCape(new ImageValidation.Dimensions(128, 64)));
        expectCapeInvalid(63, 32);
        expectCapeInvalid(64, 33);
        expectCapeInvalid(2048, 1024);
        expectCapeInvalid(1024, 4096);
    }

    @Test
    public void skyRules() throws Exception {
        ImageValidation.validateSky(new ImageValidation.Dimensions(1536, 1024));
        try {
            ImageValidation.validateSky(new ImageValidation.Dimensions(1024, 1024));
            fail();
        } catch (ImageValidation.InvalidImageException expected) {
            // expected
        }
        try {
            ImageValidation.validateSky(new ImageValidation.Dimensions(6144, 4096));
            fail();
        } catch (ImageValidation.InvalidImageException expected) {
            // expected
        }
    }

    private static void expectInvalid(File f, long max) {
        try {
            ImageValidation.readPngHeader(f, max);
            fail("expected invalid: " + f);
        } catch (ImageValidation.InvalidImageException expected) {
            // expected
        }
    }

    private static void expectCapeInvalid(int w, int h) {
        try {
            ImageValidation.validateCape(new ImageValidation.Dimensions(w, h));
            fail("expected invalid cape " + w + "x" + h);
        } catch (ImageValidation.InvalidImageException expected) {
            // expected
        }
    }
}
