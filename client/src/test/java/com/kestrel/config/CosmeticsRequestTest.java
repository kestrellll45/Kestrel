package com.kestrel.config;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class CosmeticsRequestTest {
    // Shape written by the Rust launcher (kestrel-core/src/cosmetics.rs).
    private static final String REQ = "{\"revision\": 1729760000123,"
            + "\"cosmetics\": {\"capesEnabled\": true, \"cape\": \"custom:my_cape.png\", \"animatedCapes\": false},"
            + "\"sky\": {\"enabled\": true, \"preset\": \"custom:space.png\", \"brightness\": 0.8, \"rotate\": false},"
            + "\"killEffects\": {\"enabled\": true, \"effect\": \"hearts\", \"intensity\": 1.5, \"sounds\": false, \"color\": 4283215696}}";

    @Test
    public void appliesOncePerRevision() {
        ClientConfig c = new ClientConfig();
        c.validate();
        c.hudElement("fps", true, 0.3f, 0.4f);
        assertTrue(CosmeticsRequest.apply(c, REQ));
        assertEquals("custom:my_cape.png", c.cosmetics.cape);
        assertFalse(c.cosmetics.animatedCapes);
        assertTrue(c.sky.enabled);
        assertEquals("custom:space.png", c.sky.preset);
        assertEquals(0.8f, c.sky.brightness, 1e-6f);
        assertEquals("hearts", c.killEffects.effect);
        assertEquals(1.5f, c.killEffects.intensity, 1e-6f);
        assertFalse(c.killEffects.sounds);
        assertEquals(0xFF4CAF50, c.killEffects.color); // 4283215696 unsigned
        assertEquals(1729760000123L, c.lastLauncherCosmeticsRevision);
        assertEquals(0.3f, c.hud.get("fps").x, 0f); // untouched

        // In-game change after the request must not be undone by the same revision.
        c.cosmetics.cape = "ember";
        assertFalse(CosmeticsRequest.apply(c, REQ));
        assertEquals("ember", c.cosmetics.cape);
    }

    @Test
    public void ignoresWrongTypesAndClamps() {
        ClientConfig c = new ClientConfig();
        c.validate();
        assertTrue(CosmeticsRequest.apply(c, "{\"revision\":5,\"sky\":{\"enabled\":\"yes\",\"brightness\":99},"
                + "\"killEffects\":{\"effect\":7,\"intensity\":-4}}"));
        assertFalse(c.sky.enabled);
        assertEquals(1.0f, c.sky.brightness, 0f);
        assertEquals("burst", c.killEffects.effect);
        assertEquals(0.25f, c.killEffects.intensity, 0f);
        assertFalse(CosmeticsRequest.apply(c, "[1,2]"));
        assertFalse(CosmeticsRequest.apply(c, "{\"revision\":3}"));
    }

    @Test
    public void appliesFullbrightFromLauncher() {
        ClientConfig c = new ClientConfig();
        c.validate();
        assertFalse(c.qol.fullbright);
        assertTrue(CosmeticsRequest.apply(c, "{\"revision\": 5, \"qol\": {\"fullbright\": true}}"));
        assertTrue(c.qol.fullbright);
        assertTrue(CosmeticsRequest.apply(c, "{\"revision\": 6, \"qol\": {\"fullbright\": false}}"));
        assertFalse(c.qol.fullbright);
        // Older launchers omit "qol"; a wrong type is ignored. Both keep the current value.
        c.qol.fullbright = true;
        assertTrue(CosmeticsRequest.apply(c, "{\"revision\": 7}"));
        assertTrue(c.qol.fullbright);
        assertTrue(CosmeticsRequest.apply(c, "{\"revision\": 8, \"qol\": {\"fullbright\": \"yes\"}}"));
        assertTrue(c.qol.fullbright);
        assertTrue(CosmeticsRequest.apply(c, "{\"revision\": 9, \"qol\": null}"));
        assertTrue(c.qol.fullbright);
    }

    @Test
    public void showAssignedCapesIsCopiedAndDefaultsToOn() {
        ClientConfig c = new ClientConfig();
        c.validate();
        assertTrue(c.cosmetics.showAssignedCapes);
        assertTrue(CosmeticsRequest.apply(c, "{\"revision\": 5, \"cosmetics\": {\"cape\": \"teto\", \"showAssignedCapes\": false}}"));
        assertFalse(c.cosmetics.showAssignedCapes);
        assertEquals("teto", c.cosmetics.cape); // stored as-is; CapeManager enforces ownership at render time
        // Older launchers do not send the field: keep the current value.
        assertTrue(CosmeticsRequest.apply(c, "{\"revision\": 6, \"cosmetics\": {\"cape\": \"ember\"}}"));
        assertFalse(c.cosmetics.showAssignedCapes);
    }
}
