package com.kestrel.config;

import com.google.gson.Gson;
import org.junit.Rule;
import org.junit.Test;
import org.junit.rules.TemporaryFolder;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class ConfigManagerTest {
    @Rule
    public TemporaryFolder tmp = new TemporaryFolder();

    private static void write(File f, String s) throws Exception {
        Files.write(f.toPath(), s.getBytes(StandardCharsets.UTF_8));
    }

    @Test
    public void missingFileGivesDefaults() {
        ClientConfig c = ConfigManager.readOrDefault(new File(tmp.getRoot(), "nope.json"), new Gson());
        assertEquals(ClientConfig.CURRENT_VERSION, c.configVersion);
        assertTrue(c.killEffects.enabled);
    }

    @Test
    public void partialFileKeepsDefaultsAndClamps() throws Exception {
        File f = tmp.newFile("config.json");
        write(f, "{\"crosshair\":{\"length\":999,\"custom\":true},\"killEffects\":{\"volume\":-3}}");
        ClientConfig c = ConfigManager.readOrDefault(f, new Gson());
        assertTrue(c.crosshair.custom);
        assertEquals(20, c.crosshair.length);
        assertEquals(0f, c.killEffects.volume, 0f);
        assertEquals("burst", c.killEffects.effect); // default retained
        assertEquals(1, c.configVersion);
    }

    @Test
    public void malformedFileIsBackedUpNotLost() throws Exception {
        File f = tmp.newFile("config.json");
        write(f, "{ this is not json");
        ClientConfig c = ConfigManager.readOrDefault(f, new Gson());
        assertFalse(c.crosshair.custom);
        File bad = new File(tmp.getRoot(), "config.json.bad");
        assertTrue(bad.isFile());
        assertEquals("{ this is not json", new String(Files.readAllBytes(bad.toPath()), StandardCharsets.UTF_8));
    }

    @Test
    public void wrongTypesFallBackToDefaults() throws Exception {
        File f = tmp.newFile("config.json");
        write(f, "{\"crosshair\":\"oops\"}");
        ClientConfig c = ConfigManager.readOrDefault(f, new Gson());
        assertFalse(c.crosshair.custom);
    }

    @Test
    public void saveRoundTripIsAtomic() throws Exception {
        File f = new File(tmp.getRoot(), "sub/config.json");
        ConfigManager m = new ConfigManager(f);
        m.load();
        m.get().sky.enabled = true;
        m.get().hudElement("fps", true, 0.2f, 0.3f);
        m.saveNow();
        assertTrue(f.isFile());
        assertFalse(new File(f.getParentFile(), "config.json.tmp").exists());
        ConfigManager m2 = new ConfigManager(f);
        m2.load();
        assertTrue(m2.get().sky.enabled);
        assertEquals(0.2f, m2.get().hud.get("fps").x, 0f);
        m.shutdown();
        m2.shutdown();
    }
}
