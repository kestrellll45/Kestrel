package com.kestrel.rendering;

import org.junit.After;
import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class FullbrightTest {
    @After
    public void reset() {
        Fullbright.setEnabled(false);
    }

    /** Port of the 1.8.9 lightmap gamma step (EntityRenderer.updateLightmap) for one channel. */
    private static float lightmapChannel(float c, float gamma) {
        float inv = 1.0f - c;
        float curve = 1.0f - inv * inv * inv * inv;
        float out = c * (1.0f - gamma) + curve * gamma;
        out = out * 0.96f + 0.03f;
        return Math.max(0f, Math.min(1f, out));
    }

    @Test
    public void disabledPassesThePlayersGammaThrough() {
        assertFalse(Fullbright.isEnabled());
        for (float g : new float[]{0f, 0.25f, 0.5f, 1f}) {
            assertEquals(g, Fullbright.lightmapGamma(g), 0f);
        }
    }

    @Test
    public void enabledMakesTheDarkestLightmapCellFullyBright() {
        // Darkest value the lightmap can hold before the gamma step (floored to 0.03).
        float darkest = 0.03f;
        assertTrue("vanilla max brightness is still dark", lightmapChannel(darkest, 1.0f) < 0.2f);
        Fullbright.setEnabled(true);
        assertEquals(1.0f, lightmapChannel(darkest, Fullbright.lightmapGamma(0.0f)), 0f);
        assertEquals(1.0f, lightmapChannel(darkest, Fullbright.lightmapGamma(1.0f)), 0f);
    }

    @Test
    public void noCumulativeChangeAndExactRestore() {
        float configured = 0.37f;
        Fullbright.setEnabled(true);
        float first = Fullbright.lightmapGamma(configured);
        for (int i = 0; i < 1000; i++) {
            assertEquals(first, Fullbright.lightmapGamma(configured), 0f); // stateless, every frame
        }
        Fullbright.setEnabled(true); // repeated enable is harmless
        assertEquals(first, Fullbright.lightmapGamma(configured), 0f);
        Fullbright.setEnabled(false);
        assertEquals(configured, Fullbright.lightmapGamma(configured), 0f);
        // Many toggles never drift.
        for (int i = 0; i < 50; i++) {
            Fullbright.setEnabled(i % 2 == 0);
        }
        Fullbright.setEnabled(false);
        assertEquals(configured, Fullbright.lightmapGamma(configured), 0f);
    }

    @Test
    public void nanGammaIsHandled() {
        Fullbright.setEnabled(true);
        assertEquals(Fullbright.GAMMA, Fullbright.lightmapGamma(Float.NaN), 0f);
    }
}
