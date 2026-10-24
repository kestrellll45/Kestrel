package com.kestrel.performance;

import com.kestrel.KestrelMod;
import com.kestrel.config.Preset;
import net.minecraft.client.Minecraft;
import net.minecraft.client.settings.GameSettings;
import net.minecraftforge.fml.common.gameevent.TickEvent;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

/**
 * Performance diagnostics and preset application to vanilla video settings.
 *
 * <p>Vanilla settings are only changed when the user has opted in
 * ("Also apply video settings" in the Performance tab), and only the
 * settings listed in {@link #applyVanillaSettings} are touched.
 */
public final class PerformanceManager {
    private final FrameStats frameStats = new FrameStats(1000);

    @SubscribeEvent
    public void onRenderTick(TickEvent.RenderTickEvent event) {
        if (event.phase != TickEvent.Phase.END) return;
        frameStats.frame(System.nanoTime());
        frameStats.update(Minecraft.getSystemTime());
    }

    public FrameStats frameStats() {
        return frameStats;
    }

    /**
     * Apply the preset's vanilla video settings. Changes: render distance,
     * particles, graphics (fast/fancy), entity shadows. Saves options.txt.
     */
    public void applyVanillaSettings(Preset p) {
        Minecraft mc = Minecraft.getMinecraft();
        GameSettings gs = mc.gameSettings;
        boolean oldFancy = gs.fancyGraphics;
        switch (p) {
            case POTATO:
                gs.renderDistanceChunks = 6;
                gs.particleSetting = 2; // minimal
                gs.fancyGraphics = false;
                gs.entityShadows = false;
                break;
            case COMPETITIVE:
                gs.renderDistanceChunks = 8;
                gs.particleSetting = 1; // decreased
                gs.fancyGraphics = false;
                gs.entityShadows = false;
                break;
            case VISUAL:
                gs.renderDistanceChunks = 12;
                gs.particleSetting = 0; // all
                gs.fancyGraphics = true;
                gs.entityShadows = true;
                break;
            default:
                return;
        }
        if (oldFancy != gs.fancyGraphics && mc.renderGlobal != null && mc.theWorld != null) {
            // Leaf transparency etc. depend on this; vanilla reloads renderers too.
            mc.renderGlobal.loadRenderers();
        }
        gs.saveOptions();
        KestrelMod.LOG.info("[Kestrel] applied {} video settings", p.id);
        frameStats.reset();
    }
}
