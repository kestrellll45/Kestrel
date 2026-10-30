package com.kestrel.cosmetics;

import org.junit.Test;

import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

public class MojangCapesTest {
    private static MojangCapes bundled() throws Exception {
        return MojangCapes.parse(new String(GifDecoderTest.resource("/assets/kestrel/mojang_capes.json"), StandardCharsets.UTF_8));
    }

    @Test
    public void bundledListHasTheTwelveRequestedCapes() throws Exception {
        List<String> ids = new ArrayList<String>();
        for (MojangCapes.Entry e : bundled().entries()) ids.add(e.id);
        String[] expected = {"minecon_2011", "minecon_2012", "minecon_2013", "minecon_2015", "minecon_2016",
                "realms_mapmaker", "crafter", "translator", "mojang_classic", "mojang", "cobalt", "mojira_moderator"};
        assertEquals(java.util.Arrays.asList(expected), ids);
        MojangCapes.Entry m = bundled().bySelection("mojang:mojira_moderator");
        assertEquals("Mojira Moderator", m.name);
    }

    @Test
    public void urlsOnlyPointAtMojangsTextureServer() throws Exception {
        for (MojangCapes.Entry e : bundled().entries()) {
            assertTrue(MojangCapes.isTextureId(e.textureId));
            assertEquals("https://textures.minecraft.net/texture/" + e.textureId, e.httpsUrl());
            assertEquals("http://textures.minecraft.net/texture/" + e.textureId, e.httpUrl());
            assertEquals(e.textureId + ".png", e.fileName());
            assertEquals("mojang:" + e.id, e.selectionId());
        }
    }

    @Test
    public void selectionLookup() throws Exception {
        MojangCapes m = bundled();
        assertNotNull(m.bySelection("mojang:minecon_2011"));
        assertNull(m.bySelection("minecon_2011"));
        assertNull(m.bySelection("mojang:nope"));
        assertNull(m.bySelection("custom:x.png"));
        assertNull(m.bySelection(null));
    }

    @Test
    public void rejectsMalformedLists() {
        String ok = "\"textureId\":\"" + repeat('a', 64) + "\"";
        String[] bad = {
                "[]",
                "{\"capes\":[{\"id\":\"Bad\",\"name\":\"x\"," + ok + "}]}",
                "{\"capes\":[{\"id\":\"a\",\"name\":\"x\",\"textureId\":\"../../etc\"}]}",
                "{\"capes\":[{\"id\":\"a\",\"name\":\"x\",\"textureId\":\"" + repeat('A', 64) + "\"}]}",
                "{\"capes\":[{\"id\":\"a\",\"name\":\"x\",\"textureId\":\"" + repeat('a', 65) + "\"}]}",
                "{\"capes\":[{\"id\":\"a\",\"name\":\"x\",\"textureId\":\"" + repeat('a', 31) + "\"}]}",
                "{\"capes\":[{\"id\":\"a\",\"name\":\"x\"," + ok + "},{\"id\":\"a\",\"name\":\"y\",\"textureId\":\"" + repeat('b', 64) + "\"}]}",
                "{\"capes\":[{\"id\":\"a\",\"name\":\"x\"," + ok + "},{\"id\":\"b\",\"name\":\"y\"," + ok + "}]}",
        };
        for (String json : bad) {
            try {
                MojangCapes.parse(json);
                fail("accepted: " + json);
            } catch (IllegalArgumentException | IllegalStateException expected) {
                // ok
            }
        }
    }

    @Test
    public void cachedTextureIsUsedWithoutTheNetwork() throws Exception {
        File dir = java.nio.file.Files.createTempDirectory("kestrel-mojang").toFile();
        MojangCapes.Entry e = bundled().entries().get(0);
        File cached = new File(dir, e.fileName());
        FileOutputStream out = new FileOutputStream(cached);
        out.write(new byte[] {1, 2, 3});
        out.close();
        assertEquals(cached.getAbsolutePath(), CapeManager.mojangFile(dir, e).getAbsolutePath());
        cached.delete();
        dir.delete();
    }

    private static String repeat(char c, int n) {
        StringBuilder b = new StringBuilder(n);
        for (int i = 0; i < n; i++) b.append(c);
        return b.toString();
    }
}
