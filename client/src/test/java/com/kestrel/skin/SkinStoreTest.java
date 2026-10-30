package com.kestrel.skin;

import org.junit.Rule;
import org.junit.Test;
import org.junit.rules.TemporaryFolder;

import javax.imageio.ImageIO;
import java.awt.image.BufferedImage;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.Arrays;
import java.util.List;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

public class SkinStoreTest {
    @Rule
    public TemporaryFolder tmp = new TemporaryFolder();

    static byte[] png(int w, int h, boolean slim) throws IOException {
        BufferedImage img = new BufferedImage(w, h, BufferedImage.TYPE_INT_ARGB);
        for (int y = 0; y < h; y++) {
            for (int x = 0; x < w; x++) {
                boolean empty = slim && x >= 54 && x < 56 && y >= 20 && y < 32;
                int a = empty || y >= 32 ? 0 : 255;
                img.setRGB(x, y, (a << 24) | (x << 16) | (y << 8) | 7);
            }
        }
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        ImageIO.write(img, "png", out);
        return out.toByteArray();
    }

    private SkinStore store() throws IOException {
        return new SkinStore(new File(tmp.getRoot(), "skins"));
    }

    @Test
    public void validatesFormatAndSize() throws Exception {
        assertEquals(SkinStore.MODEL_CLASSIC, SkinStore.validate(png(64, 64, false)).suggestedModel);
        assertEquals(SkinStore.MODEL_SLIM, SkinStore.validate(png(64, 64, true)).suggestedModel);
        SkinStore.Validated legacy = SkinStore.validate(png(64, 32, false));
        assertTrue(legacy.legacy);
        assertEquals(SkinStore.MODEL_CLASSIC, legacy.suggestedModel);
        for (byte[] bad : Arrays.asList(png(128, 128, false), png(64, 48, false), "GIF89a......................".getBytes(StandardCharsets.US_ASCII), new byte[5])) {
            try {
                SkinStore.validate(bad);
                fail("accepted invalid skin");
            } catch (SkinStore.SkinException expected) {
                // message is shown to the user
                assertFalse(expected.getMessage().isEmpty());
            }
        }
        byte[] big = Arrays.copyOf(png(64, 64, false), (int) SkinStore.MAX_SKIN_BYTES + 1);
        try {
            SkinStore.validate(big);
            fail();
        } catch (SkinStore.SkinException e) {
            assertTrue(e.getMessage().contains("too large"));
        }
    }

    @Test
    public void importStoresBytesUnchangedDedupesAndPersists() throws Exception {
        SkinStore s = store();
        SkinStore.State st = s.load();
        assertNull(st.active);
        byte[] bytes = png(64, 64, true);
        SkinStore.Entry e = s.importBytes(st, bytes, "My\u0007 \u00a7askin", "my.png", null);
        assertTrue(SkinStore.isValidFileName(e.file));
        assertEquals("My skin", e.name);
        assertEquals(SkinStore.MODEL_SLIM, e.model);
        assertArrayEquals(bytes, Files.readAllBytes(s.skinFile(e.file).toPath()));
        SkinStore.Entry again = s.importBytes(st, bytes, "dup", "", "classic");
        assertEquals(e.file, again.file);
        assertEquals(1, st.skins.size());

        st.active = e.file;
        s.save(st);
        long r1 = st.revision;
        SkinStore.State loaded = s.load();
        assertEquals(e.file, loaded.active);
        assertEquals(SkinStore.MODEL_CLASSIC, loaded.activeEntry().model);
        s.save(st);
        assertTrue(st.revision > r1);

        st.active = null; // restore account skin
        s.save(st);
        assertNull(s.load().active);
        assertEquals(1, s.load().skins.size());

        s.remove(st, e.file);
        assertTrue(st.skins.isEmpty());
        assertFalse(s.skinFile(e.file).exists());
    }

    @Test
    public void legacyIsAlwaysClassicAndRejectsWriteNothing() throws Exception {
        SkinStore s = store();
        SkinStore.State st = s.load();
        assertEquals(SkinStore.MODEL_CLASSIC, s.importBytes(st, png(64, 32, false), "old", "", "slim").model);
        SkinStore s2 = new SkinStore(new File(tmp.getRoot(), "other"));
        SkinStore.State st2 = s2.load();
        try {
            s2.importBytes(st2, png(32, 32, false), "x", "", null);
            fail();
        } catch (SkinStore.SkinException expected) {
            assertTrue(st2.skins.isEmpty());
            assertFalse(s2.dir().exists());
        }
    }

    @Test
    public void readsTheLaunchersFileAndRepairsUntrustedState() throws Exception {
        SkinStore s = store();
        assertTrue(s.dir().mkdirs());
        Files.write(new File(s.dir(), "0123456789abcdef.png").toPath(), png(64, 64, false));
        // Exactly what kestrel_core::skins::save_state writes (serde, camelCase).
        String launcher = "{\n  \"version\": 1,\n  \"active\": \"0123456789abcdef.png\",\n  \"skins\": [\n    {\n"
                + "      \"file\": \"0123456789abcdef.png\",\n      \"name\": \"Notch\",\n      \"model\": \"slim\",\n"
                + "      \"source\": \"https://namemc.com/profile/Notch\",\n      \"added\": 1793300000000\n    }\n  ],\n"
                + "  \"revision\": 1793300000001,\n  \"updatedBy\": \"launcher\"\n}";
        Files.write(s.stateFile().toPath(), launcher.getBytes(StandardCharsets.UTF_8));
        SkinStore.State st = s.load();
        assertEquals("0123456789abcdef.png", st.active);
        assertEquals("Notch", st.activeEntry().name);
        assertEquals(SkinStore.MODEL_SLIM, st.activeEntry().model);
        assertEquals(1793300000001L, st.revision);
        s.save(st);
        assertTrue(s.load().revision > 1793300000001L);
        assertEquals("game", s.load().updatedBy);

        String evil = "{\"active\":\"../../evil.png\",\"skins\":[{\"file\":\"../../evil.png\"},"
                + "{\"file\":\"0123456789abcdef.png\",\"model\":\"SLIM\"},{\"file\":\"0123456789abcdef.png\"},"
                + "{\"file\":\"fedcba9876543210.png\"},null],\"revision\":\"oops\"}";
        Files.write(s.stateFile().toPath(), evil.getBytes(StandardCharsets.UTF_8));
        SkinStore.State repaired = s.load();
        assertNull(repaired.active); // type error in revision -> treated as no file at all
        Files.write(s.stateFile().toPath(), evil.replace("\"oops\"", "5").getBytes(StandardCharsets.UTF_8));
        repaired = s.load();
        assertEquals(1, repaired.skins.size());
        assertEquals(SkinStore.MODEL_SLIM, repaired.skins.get(0).model);
        assertNull(repaired.active);
        Files.write(s.stateFile().toPath(), "not json".getBytes(StandardCharsets.UTF_8));
        assertTrue(s.load().skins.isEmpty());
    }

    @Test
    public void importFolderImportsValidFilesAndReportsOthers() throws Exception {
        SkinStore s = store();
        assertTrue(s.importDir().mkdirs());
        Files.write(new File(s.importDir(), "good.png").toPath(), png(64, 64, false));
        Files.write(new File(s.importDir(), "bad.png").toPath(), png(16, 16, false));
        Files.write(new File(s.importDir(), "notes.txt").toPath(), new byte[] {1});
        SkinStore.State st = s.load();
        List<String> problems = s.importFolder(st);
        assertEquals(1, st.skins.size());
        assertEquals("good", st.skins.get(0).name);
        assertEquals(1, problems.size());
        assertTrue(problems.get(0).startsWith("bad.png"));
        assertTrue("originals are left in place", new File(s.importDir(), "good.png").isFile());
    }

    @Test
    public void fileNamesCannotEscape() {
        assertTrue(SkinStore.isValidFileName("0123456789abcdef.png"));
        for (String bad : new String[] {"0123456789ABCDEF.png", "../0123456789abcdef.png", "0123456789abcde.png", "0123456789abcdef.gif", "", null}) {
            assertFalse(bad, SkinStore.isValidFileName(bad));
        }
    }
}
