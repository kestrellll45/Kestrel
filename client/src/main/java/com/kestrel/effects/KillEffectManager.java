package com.kestrel.effects;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;
import net.minecraft.client.audio.PositionedSoundRecord;
import net.minecraft.client.entity.EntityPlayerSP;
import net.minecraft.client.multiplayer.WorldClient;
import net.minecraft.util.ResourceLocation;
import net.minecraft.util.Vec3;
import net.minecraftforge.client.event.RenderWorldLastEvent;
import net.minecraftforge.event.world.WorldEvent;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;

import java.util.ArrayList;
import java.util.List;

/**
 * Runs kill effects. Purely visual and client-side: nothing is sent to the
 * server and nothing affects gameplay. Concurrency and particle counts are
 * bounded so a multi-kill can never flood the particle system.
 */
public final class KillEffectManager {
    private static final int MAX_CONCURRENT = 6;

    private final KestrelClient client;
    private final List<ActiveEffect> active = new ArrayList<ActiveEffect>();

    public KillEffectManager(KestrelClient client) {
        this.client = client;
    }

    /** Start the configured effect at a position (e.g. where the victim died). */
    public void trigger(double x, double y, double z) {
        ClientConfig.KillEffects cfg = client.config().killEffects;
        if (!cfg.enabled) return;
        play(KillEffectType.byId(cfg.effect), x, y, z, cfg);
    }

    /** Show the selected effect a few blocks in front of the player (settings preview). */
    public void preview() {
        EntityPlayerSP p = Minecraft.getMinecraft().thePlayer;
        if (p == null) return;
        Vec3 look = p.getLookVec();
        ClientConfig.KillEffects cfg = client.config().killEffects;
        play(KillEffectType.byId(cfg.effect), p.posX + look.xCoord * 4, p.posY, p.posZ + look.zCoord * 4, cfg);
    }

    private void play(KillEffectType type, double x, double y, double z, ClientConfig.KillEffects cfg) {
        if (Minecraft.getMinecraft().theWorld == null) return;
        if (active.size() >= MAX_CONCURRENT) {
            active.remove(0);
        }
        active.add(Effects.create(type, x, y, z, cfg.intensity, cfg.maxParticles, cfg.color));
        if (cfg.sounds && cfg.volume > 0) {
            Minecraft.getMinecraft().getSoundHandler().playSound(new PositionedSoundRecord(
                    new ResourceLocation(type.sound), cfg.volume, type.pitch, (float) x, (float) y, (float) z));
        }
    }

    /** Play a specific effect in front of the player (in-game picker preview). */
    public void preview(KillEffectType type) {
        EntityPlayerSP p = Minecraft.getMinecraft().thePlayer;
        if (p == null) return;
        Vec3 look = p.getLookVec();
        play(type, p.posX + look.xCoord * 4, p.posY, p.posZ + look.zCoord * 4, client.config().killEffects);
    }

    @SubscribeEvent
    public void onTick(TickEvent.ClientTickEvent event) {
        if (event.phase != TickEvent.Phase.END || active.isEmpty()) return;
        WorldClient w = Minecraft.getMinecraft().theWorld;
        if (w == null) {
            active.clear();
            return;
        }
        for (int i = active.size() - 1; i >= 0; i--) {
            ActiveEffect e = active.get(i);
            e.tick(w);
            if (e.finished()) active.remove(i);
        }
    }

    @SubscribeEvent
    public void onRenderWorld(RenderWorldLastEvent event) {
        if (active.isEmpty()) return;
        float pt = event.partialTicks;
        double cx = RenderUtil.camX(pt);
        double cy = RenderUtil.camY(pt);
        double cz = RenderUtil.camZ(pt);
        for (int i = 0; i < active.size(); i++) {
            active.get(i).render(pt, cx, cy, cz);
        }
    }

    @SubscribeEvent
    public void onWorldUnload(WorldEvent.Unload event) {
        if (event.world.isRemote) active.clear();
    }
}
