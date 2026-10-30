package com.kestrel.rendering;

import org.junit.Test;

import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.HashSet;
import java.util.Set;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

public class SkyCatalogTest {
    private static byte[] resource(String path) throws Exception {
        InputStream in = SkyCatalogTest.class.getResourceAsStream(path);
        assertNotNull("missing resource " + path, in);
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

    private static SkyCatalog bundled() throws Exception {
        return SkyCatalog.parse(new String(resource("/assets/kestrel/skies.json"), StandardCharsets.UTF_8));
    }

    @Test
    public void bundledCatalogKeepsTheOriginalSkiesAndHasAtLeast25More() throws Exception {
        SkyCatalog c = bundled();
        for (String id : new String[] {"dusk", "starry", "nebula"}) {
            assertNotNull(id + " must stay (saved settings refer to it)", c.get(id));
        }
        assertTrue("expected >= 28 skies, got " + c.entries().size(), c.entries().size() >= 28);
    }

    @Test
    public void everyBundledSkyHasAValidAtlasAndThumbnail() throws Exception {
        Set<String> used = new HashSet<String>();
        for (SkyCatalog.Entry e : bundled().entries()) {
            used.add(e.category);
            java.awt.image.BufferedImage atlas = javax.imageio.ImageIO.read(
                    new java.io.ByteArrayInputStream(resource("/assets/kestrel/textures/" + e.texture())));
            assertEquals(e.id + ": 3x2 grid", atlas.getWidth() * 2, atlas.getHeight() * 3);
            java.awt.image.BufferedImage thumb = javax.imageio.ImageIO.read(
                    new java.io.ByteArrayInputStream(resource("/assets/kestrel/textures/" + e.thumbnail())));
            assertEquals(e.id + ": 3:1 thumbnail", thumb.getWidth(), thumb.getHeight() * 3);
        }
        for (String cat : SkyCatalog.CATEGORIES) {
            assertTrue("category " + cat + " is empty", used.contains(cat));
        }
    }

    @Test
    public void unknownIdsAreNotFound() throws Exception {
        assertNull(bundled().get("does_not_exist"));
    }

    @Test
    public void rejectsMalformedEntries() {
        String[] bad = {
                "[]",
                "{}",
                "{\"skies\":[{\"id\":\"Bad Id\",\"name\":\"x\",\"category\":\"space\"}]}",
                "{\"skies\":[{\"id\":\"a\",\"name\":\"x\",\"category\":\"nope\"}]}",
                "{\"skies\":[{\"id\":\"a\",\"name\":\"\",\"category\":\"space\"}]}",
                "{\"skies\":[{\"id\":\"a\",\"name\":\"x\",\"category\":\"space\"},{\"id\":\"a\",\"name\":\"y\",\"category\":\"space\"}]}",
        };
        for (String json : bad) {
            try {
                SkyCatalog.parse(json);
                fail("accepted: " + json);
            } catch (IllegalArgumentException | IllegalStateException expected) {
                // ok
            }
        }
    }

    @Test
    public void categoryLabels() {
        assertEquals("Clouds & Aurora", SkyCatalog.categoryLabel("atmospheric"));
        assertEquals("Space", SkyCatalog.categoryLabel("space"));
    }
}
