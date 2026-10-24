package com.kestrel.config;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

public class PresetApplierTest {
    @Test
    public void presetsKeepUnrelatedPreferences() {
        ClientConfig c = new ClientConfig();
        c.hudElement("fps", false, 0.3f, 0.4f).scale = 2.0f;
        c.crosshair.custom = true;
        c.crosshair.color = 0xFF00FF00;
        c.cosmetics.cape = "ember";
        c.sky.preset = "starry";

        PresetApplier.apply(c, Preset.POTATO);
        assertFalse(c.sky.enabled);
        assertFalse(c.cosmetics.animatedCapes);
        assertEquals("potato", c.activePreset);

        // Unrelated preferences survive.
        assertEquals(0.3f, c.hud.get("fps").x, 0f);
        assertEquals(2.0f, c.hud.get("fps").scale, 0f);
        assertTrue(c.crosshair.custom);
        assertEquals(0xFF00FF00, c.crosshair.color);
        assertEquals("ember", c.cosmetics.cape);
        assertEquals("starry", c.sky.preset);

        PresetApplier.apply(c, Preset.COMPETITIVE);
        assertTrue(c.hud.get("fps").enabled);
        assertTrue(c.hud.get("cps").enabled);
        assertTrue(c.hud.get("keystrokes").enabled);
        assertEquals(0.3f, c.hud.get("fps").x, 0f);

        PresetApplier.apply(c, Preset.VISUAL);
        assertTrue(c.sky.enabled);
        assertTrue(c.cosmetics.animatedCapes);
        assertEquals(150, c.killEffects.maxParticles);
    }

    @Test
    public void launcherRequestAppliedOncePerDistinctValue() {
        ClientConfig c = new ClientConfig();
        assertEquals(Preset.VISUAL, PresetApplier.applyLauncherRequest(c, "visual"));
        // User changes something in-game; the same launcher request must not reset it.
        c.sky.enabled = false;
        assertNull(PresetApplier.applyLauncherRequest(c, "visual"));
        assertFalse(c.sky.enabled);
        assertEquals(Preset.POTATO, PresetApplier.applyLauncherRequest(c, "POTATO"));
        assertNull(PresetApplier.applyLauncherRequest(c, null));
        assertNull(PresetApplier.applyLauncherRequest(c, "ultra"));
    }
}
