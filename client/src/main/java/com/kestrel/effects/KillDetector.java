package com.kestrel.effects;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import net.minecraft.client.Minecraft;
import net.minecraft.entity.Entity;
import net.minecraft.entity.EntityLivingBase;
import net.minecraft.entity.player.EntityPlayer;
import net.minecraft.world.World;
import net.minecraftforge.client.event.ClientChatReceivedEvent;
import net.minecraftforge.event.entity.player.AttackEntityEvent;
import net.minecraftforge.event.world.WorldEvent;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;

import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;

/**
 * Detects kills by the local player. Separate from the effect layer so new
 * detection strategies (e.g. per-server message formats) can be added.
 *
 * <p>Two strategies, both requiring that <em>you</em> hit the victim recently
 * (we never fire because some nearby entity disappeared):
 * <ol>
 *   <li>Health: a recently-hit entity's synced health reaches 0 (works on
 *   servers that sync health and kill players normally, and singleplayer).</li>
 *   <li>Chat: a kill-feed line names both the recently-hit player and you
 *   (works on minigame servers that respawn players instead of killing them).</li>
 * </ol>
 * Each victim triggers at most one effect per {@link #COOLDOWN_MS}.
 */
public final class KillDetector {
    static final long HEALTH_WINDOW_MS = 3000;
    static final long CHAT_WINDOW_MS = 10000;
    static final long COOLDOWN_MS = 3000;

    private static final class Target {
        final int entityId;
        final String name;
        long lastHitMs;
        double x;
        double y;
        double z;

        Target(int entityId, String name) {
            this.entityId = entityId;
            this.name = name;
        }
    }

    private final KestrelClient client;
    private final KillEffectManager effects;
    private final Map<Integer, Target> targets = new HashMap<Integer, Target>();
    private final Map<String, Long> cooldowns = new HashMap<String, Long>();

    public KillDetector(KestrelClient client, KillEffectManager effects) {
        this.client = client;
        this.effects = effects;
    }

    @SubscribeEvent
    public void onAttack(AttackEntityEvent event) {
        Minecraft mc = Minecraft.getMinecraft();
        if (event.entityPlayer != mc.thePlayer || !(event.target instanceof EntityLivingBase)) return;
        ClientConfig.KillEffects cfg = client.config().killEffects;
        if (!cfg.enabled) return;
        boolean isPlayer = event.target instanceof EntityPlayer;
        if (!isPlayer && !cfg.includeMobs) return;
        Entity t = event.target;
        Target tg = targets.get(t.getEntityId());
        if (tg == null) {
            tg = new Target(t.getEntityId(), isPlayer ? t.getName() : null);
            targets.put(t.getEntityId(), tg);
        }
        tg.lastHitMs = System.currentTimeMillis();
        tg.x = t.posX;
        tg.y = t.posY;
        tg.z = t.posZ;
    }

    @SubscribeEvent
    public void onTick(TickEvent.ClientTickEvent event) {
        if (event.phase != TickEvent.Phase.END || targets.isEmpty()) return;
        World world = Minecraft.getMinecraft().theWorld;
        long now = System.currentTimeMillis();
        ClientConfig.KillEffects cfg = client.config().killEffects;
        Iterator<Target> it = targets.values().iterator();
        while (it.hasNext()) {
            Target tg = it.next();
            if (now - tg.lastHitMs > CHAT_WINDOW_MS) {
                it.remove();
                continue;
            }
            Entity e = world == null ? null : world.getEntityByID(tg.entityId);
            if (e instanceof EntityLivingBase) {
                EntityLivingBase l = (EntityLivingBase) e;
                tg.x = l.posX;
                tg.y = l.posY;
                tg.z = l.posZ;
                if (cfg.healthDetection && now - tg.lastHitMs <= HEALTH_WINDOW_MS && l.getHealth() <= 0.0F) {
                    fire(tg, now);
                    it.remove();
                }
            }
        }
    }

    @SubscribeEvent
    public void onChat(ClientChatReceivedEvent event) {
        if (event.type == 2 || targets.isEmpty()) return; // 2 = action bar
        ClientConfig.KillEffects cfg = client.config().killEffects;
        Minecraft mc = Minecraft.getMinecraft();
        if (!cfg.enabled || !cfg.chatDetection || mc.thePlayer == null || event.message == null) return;
        String text = event.message.getUnformattedText();
        String self = mc.thePlayer.getName();
        long now = System.currentTimeMillis();
        Iterator<Target> it = targets.values().iterator();
        while (it.hasNext()) {
            Target tg = it.next();
            if (tg.name != null && now - tg.lastHitMs <= CHAT_WINDOW_MS
                    && KillMessageMatcher.isKillMessage(text, self, tg.name)) {
                fire(tg, now);
                it.remove();
            }
        }
    }

    @SubscribeEvent
    public void onWorldUnload(WorldEvent.Unload event) {
        if (event.world.isRemote) {
            targets.clear();
            cooldowns.clear();
        }
    }

    private void fire(Target tg, long now) {
        String key = tg.name != null ? tg.name : "#" + tg.entityId;
        Long last = cooldowns.get(key);
        if (last != null && now - last < COOLDOWN_MS) return;
        cooldowns.put(key, now);
        if (cooldowns.size() > 256) cooldowns.clear(); // bounded memory
        effects.trigger(tg.x, tg.y, tg.z);
    }
}
