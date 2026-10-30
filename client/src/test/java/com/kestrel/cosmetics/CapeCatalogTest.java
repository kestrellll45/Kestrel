package com.kestrel.cosmetics;

import org.junit.Test;

import java.nio.charset.StandardCharsets;
import java.util.UUID;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

public class CapeCatalogTest {
    private static final String TETO_OWNER = "ba14933e-a7ad-4816-8696-a788cc1b3c1f";

    private static CapeCatalog bundled() throws Exception {
        return CapeCatalog.parse(new String(GifDecoderTest.resource("/assets/kestrel/capes.json"), StandardCharsets.UTF_8));
    }

    @Test
    public void bundledCatalogKeepsExistingCapesAndAddsTeto() throws Exception {
        CapeCatalog c = bundled();
        for (String id : new String[]{"ember", "glacier", "midnight", "verdant", "aurora", "teto"}) {
            assertNotNull(id, c.get(id));
        }
        for (String id : new String[]{"ember", "glacier", "midnight", "verdant", "aurora"}) {
            assertFalse(id + " must stay available to everyone", c.get(id).restricted());
            assertTrue(c.get(id).allows(null));
        }
        CapeCatalog.Entry teto = c.get("teto");
        assertTrue(teto.gif);
        assertTrue(teto.restricted());
        assertEquals(CapeLayout.Mode.FACES, teto.layout.mode);
        assertEquals("capes/teto.png", teto.fallback);
        assertEquals(1, teto.owners().size());
    }

    @Test
    public void tetoIsRestrictedToTheExactUuid() throws Exception {
        CapeCatalog.Entry teto = bundled().get("teto");
        assertTrue(teto.allows(TETO_OWNER));
        assertTrue(teto.allows("ba14933ea7ad48168696a788cc1b3c1f"));
        assertTrue(teto.allows("BA14933E-A7AD-4816-8696-A788CC1B3C1F"));
        assertTrue(teto.allows("  ba14933ea7ad48168696a788cc1b3c1f "));
        assertFalse(teto.allows("ba14933e-a7ad-4816-8696-a788cc1b3c1e")); // one digit off
        assertFalse(teto.allows("b50ad385829d3141a2167e7d7539ba7f"));     // another (offline) UUID
        assertFalse(teto.allows("bringing"));                             // a username is never accepted
        assertFalse(teto.allows(null));
        assertFalse(teto.allows(""));
        assertFalse(teto.allows("ba14933e-a7ad-4816-8696-a788cc1b3c1"));
    }

    @Test
    public void assignedCapeLookupUsesUuidObjects() throws Exception {
        CapeCatalog c = bundled();
        assertEquals("teto", c.assignedTo(UUID.fromString(TETO_OWNER)).id);
        assertNull(c.assignedTo(UUID.fromString("ba14933e-a7ad-4816-8696-a788cc1b3c1e")));
        assertNull(c.assignedTo(null));
    }

    @Test
    public void normalizeUuid() {
        assertEquals("ba14933ea7ad48168696a788cc1b3c1f", CapeCatalog.normalizeUuid(TETO_OWNER));
        assertNull(CapeCatalog.normalizeUuid("ba14933e-a7ad-4816-8696_a788cc1b3c1f"));
        assertNull(CapeCatalog.normalizeUuid("zz14933ea7ad48168696a788cc1b3c1f"));
        assertNull(CapeCatalog.normalizeUuid("ba14933ea7ad48168696a788cc1b3c1f0"));
        assertEquals(UUID.fromString(TETO_OWNER), CapeCatalog.toUuid(CapeCatalog.normalizeUuid(TETO_OWNER)));
    }

    @Test
    public void emptyOwnersListFailsClosed() {
        CapeCatalog c = CapeCatalog.parse("{\"capes\":[{\"id\":\"x\",\"name\":\"X\",\"texture\":\"capes/x.png\",\"owners\":[]}]}");
        assertTrue(c.get("x").restricted());
        assertFalse(c.get("x").allows(TETO_OWNER));
        assertFalse(c.get("x").allows(null));
    }

    @Test
    public void rejectsMalformedEntries() {
        String[] bad = {
                "[]",
                "{}",
                "{\"capes\":[{\"id\":\"x\",\"name\":\"X\",\"texture\":\"capes/x.png\",\"owners\":[\"bringing\"]}]}",
                "{\"capes\":[{\"id\":\"x\",\"name\":\"X\",\"texture\":\"../../x.png\"}]}",
                "{\"capes\":[{\"id\":\"x\",\"name\":\"X\",\"texture\":\"capes/x.bmp\"}]}",
                "{\"capes\":[{\"id\":\"X Y\",\"name\":\"X\",\"texture\":\"capes/x.png\"}]}",
                "{\"capes\":[{\"id\":\"none\",\"name\":\"X\",\"texture\":\"capes/x.png\"}]}",
                "{\"capes\":[{\"id\":\"x\",\"name\":\"X\",\"texture\":\"capes/x.png\"},{\"id\":\"x\",\"name\":\"Y\",\"texture\":\"capes/y.png\"}]}",
                "{\"capes\":[{\"id\":\"x\",\"name\":\"X\",\"texture\":\"capes/x.gif\",\"layout\":\"spiral\"}]}",
                "{\"capes\":[{\"id\":\"x\",\"name\":\"X\",\"texture\":\"capes/x.gif\",\"fill\":\"pink\"}]}",
        };
        for (String json : bad) {
            try {
                CapeCatalog.parse(json);
                fail("accepted: " + json);
            } catch (RuntimeException expected) {
                // IllegalArgumentException or a Gson exception
            }
        }
    }

    @Test
    public void bundledTetoBuildsAFullCapeFromTheGif() throws Exception {
        final CapeCatalog.Entry teto = bundled().get("teto");
        final int[] count = {0};
        GifDecoder.decode(GifDecoderTest.resource("/assets/kestrel/textures/capes/" + "teto.gif"), GifDecoder.CAPE_LIMITS,
                new GifDecoder.FrameSink() {
                    @Override
                    public void frame(int index, java.awt.image.BufferedImage canvas, int delayMs) {
                        java.awt.image.BufferedImage tex = CapeLayout.build(canvas, teto.layout);
                        assertEquals(512, tex.getWidth());
                        assertEquals(256, tex.getHeight());
                        // Both faces are fully opaque (no letterbox black at the top rows).
                        for (int face : new int[]{1, 12}) {
                            for (int y = 8; y < 17 * 8; y += 8) {
                                int p = tex.getRGB(face * 8 + 40, y);
                                assertEquals(0xFF, p >>> 24);
                            }
                            int top = tex.getRGB(face * 8 + 2, 8 + 1);
                            assertTrue(Integer.toHexString(top), ((top >> 16) & 0xFF) > 200);
                        }
                        count[0]++;
                    }
                });
        assertEquals(24, count[0]);
    }

    /** The static fallback must stay identical to the first animation frame (regenerate it if the GIF changes). */
    @Test
    public void tetoFallbackMatchesTheFirstFrame() throws Exception {
        final CapeCatalog.Entry teto = bundled().get("teto");
        java.awt.image.BufferedImage fallback = javax.imageio.ImageIO.read(
                new java.io.ByteArrayInputStream(GifDecoderTest.resource("/assets/kestrel/textures/" + teto.fallback)));
        final java.awt.image.BufferedImage[] first = new java.awt.image.BufferedImage[1];
        GifDecoder.decode(GifDecoderTest.resource("/assets/kestrel/textures/" + teto.texture), GifDecoder.CAPE_LIMITS,
                new GifDecoder.FrameSink() {
                    @Override
                    public void frame(int index, java.awt.image.BufferedImage canvas, int delayMs) {
                        if (index == 0) first[0] = CapeLayout.build(canvas, teto.layout);
                    }
                });
        assertEquals(first[0].getWidth(), fallback.getWidth());
        assertEquals(first[0].getHeight(), fallback.getHeight());
        for (int y = 0; y < fallback.getHeight(); y++) {
            for (int x = 0; x < fallback.getWidth(); x++) {
                assertEquals("pixel " + x + "," + y, first[0].getRGB(x, y), fallback.getRGB(x, y));
            }
        }
        // The fallback passes the same validation as any bundled PNG cape.
        com.kestrel.util.ImageValidation.validateCape(
                new com.kestrel.util.ImageValidation.Dimensions(fallback.getWidth(), fallback.getHeight()));
    }

    @Test
    public void bundledCatalogHasCategorisedStaticAndAnimatedCapes() throws Exception {
        CapeCatalog c = bundled();
        int animated = 0;
        int still = 0;
        java.util.Set<String> used = new java.util.HashSet<String>();
        for (CapeCatalog.Entry e : c.entries()) {
            assertTrue(e.id + ": category " + e.category, java.util.Arrays.asList(CapeCatalog.CATEGORIES).contains(e.category));
            used.add(e.category);
            if (e.animated()) animated++;
            else still++;
            // Every cape has a menu thumbnail: whole 10:16 faces side by side.
            java.awt.image.BufferedImage t = javax.imageio.ImageIO.read(
                    new java.io.ByteArrayInputStream(GifDecoderTest.resource("/assets/kestrel/textures/" + e.thumbnail())));
            int faceW = t.getHeight() * 10 / 16;
            assertEquals(e.id + ": thumbnail is whole faces", 0, t.getWidth() % faceW);
            if (e.fps > 0) {
                assertTrue(e.id + ": animated thumbnail needs frames", t.getWidth() / faceW > 1);
                assertFalse(e.id + ": fps is for PNG strips", e.gif);
            }
        }
        assertTrue("static capes: " + still, still >= 25);
        assertTrue("animated capes: " + animated, animated >= 12);
        assertEquals(java.util.Arrays.asList(CapeCatalog.CATEGORIES).size(), used.size());
    }

    @Test
    public void categoryAndFpsAreValidated() {
        String base = "{\"capes\":[{\"id\":\"x\",\"name\":\"X\",\"texture\":\"capes/x.png\"";
        CapeCatalog ok = CapeCatalog.parse(base + "}]}");
        assertEquals("static", ok.get("x").category);
        assertEquals(0, ok.get("x").fps);
        assertEquals(12, CapeCatalog.parse(base + ",\"category\":\"animated\",\"fps\":12}]}").get("x").fps);
        String[] bad = {
                base + ",\"category\":\"nope\"}]}",
                base + ",\"fps\":0}]}",
                base + ",\"fps\":61}]}",
                "{\"capes\":[{\"id\":\"x\",\"name\":\"X\",\"texture\":\"capes/x.gif\",\"fps\":10}]}",
        };
        for (String json : bad) {
            try {
                CapeCatalog.parse(json);
                fail("accepted: " + json);
            } catch (IllegalArgumentException expected) {
                // ok
            }
        }
    }
}
