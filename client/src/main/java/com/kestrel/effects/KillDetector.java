package com.kestrel.effects;

import com.kestrel.KestrelClient;
import com.kestrel.config.ClientConfig;
import net.minecraft.client.Minecraft;
import net.minecraft.entity.Entity;
import net.minecraft.entity.player.EntityPlayer;
import net.minecraft.entity.projectile.EntityArrow;
import net.minecraft.util.AxisAlignedBB;
import net.minecraft.world.World;
import net.minecraftforge.client.event.ClientChatReceivedEvent;
import net.minecraftforge.event.entity.player.AttackEntityEvent;
import net.minecraftforge.event.world.WorldEvent;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;

import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;

/**
 * Detects kills credited to the local player. Only <em>players</em> you
 * attacked yourself are ever considered; mobs, your own death, other players'
 * kills and deaths from falling, lava, fire or mobs never trigger an effect.
 *
 * <p>Two independent ways to confirm a kill, each strict on its own:
 * <ol>
 *   <li>Death message: the server's kill-feed line names the victim and makes
 *   you the killer ({@link KillMessageMatcher}). This is the server's own
 *   attribution and works on minigame servers that respawn players instead of
 *   killing them.</li>
 *   <li>Death seen in the world: the victim's health reaches 0 and
 *   {@link KillAttribution} attributes the killing blow to your hit (no other
 *   damage in between, nobody else attacking).</li>
 * </ol>
 * Whichever confirms first plays the effect, once per kill, at the victim's
 * position; the other confirmation of the same kill is ignored.
 */
public final class KillDetector {
    /** How long after your last hit a chat kill message may still credit you. */
    static final long CHAT_WINDOW_MS = 10000;
    /** After an effect, ignore further confirmations of the same victim for this long. */
    static final long COOLDOWN_MS = 6000;
    /** Other players this close to the victim count as possible attackers. */
    private static final double FOREIGN_RADIUS = 6.0;

    private static final class Target {
        final int entityId;
        final String name;
        final KillAttribution attribution = new KillAttribution();
        int lastHurtTime;
        boolean fired;
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
        if (mc.thePlayer == null || event.entityPlayer != mc.thePlayer) return;
        if (!(event.target instanceof EntityPlayer) || event.target == mc.thePlayer) return; // players only
        if (!client.config().killEffects.enabled) return;
        EntityPlayer t = (EntityPlayer) event.target;
        Target tg = targets.get(t.getEntityId());
        if (tg == null || tg.fired) {
            tg = new Target(t.getEntityId(), t.getName());
            tg.lastHurtTime = t.hurtTime;
            targets.put(t.getEntityId(), tg);
        }
        tg.attribution.ownHit(System.currentTimeMillis());
        tg.x = t.posX;
        tg.y = t.posY;
        tg.z = t.posZ;
    }

    @SubscribeEvent
    public void onTick(TickEvent.ClientTickEvent event) {
        if (event.phase != TickEvent.Phase.END || targets.isEmpty()) return;
        Minecraft mc = Minecraft.getMinecraft();
        World world = mc.theWorld;
        long now = System.currentTimeMillis();
        ClientConfig.KillEffects cfg = client.config().killEffects;
        Iterator<Target> it = targets.values().iterator();
        while (it.hasNext()) {
            Target tg = it.next();
            if (now - tg.attribution.lastOwnHitMs() > CHAT_WINDOW_MS) {
                it.remove();
                continue;
            }
            if (tg.fired) continue;
            Entity e = world == null ? null : world.getEntityByID(tg.entityId);
            if (!(e instanceof EntityPlayer)) continue;
            EntityPlayer victim = (EntityPlayer) e;
            tg.x = victim.posX;
            tg.y = victim.posY;
            tg.z = victim.posZ;
            if (victim.hurtTime > tg.lastHurtTime) {
                tg.attribution.hurt(now); // a new damage event (any cause) reached the client
            }
            tg.lastHurtTime = victim.hurtTime;
            noteForeignAttackers(world, mc.thePlayer, victim, tg, now);
            // Died but not attributable to you: only the server's death message
            // (onChat) can still credit the kill.
            if ((victim.getHealth() <= 0.0F || victim.isDead) && cfg.healthDetection
                    && tg.attribution.creditedDeath(now)) {
                fire(tg, now);
            }
        }
    }

    /** Other players swinging near the victim, or arrows not yours flying at it. */
    private static void noteForeignAttackers(World world, EntityPlayer self, EntityPlayer victim, Target tg, long now) {
        AxisAlignedBB box = victim.getEntityBoundingBox().expand(FOREIGN_RADIUS, 3.0, FOREIGN_RADIUS);
        List<Entity> near = world.getEntitiesWithinAABBExcludingEntity(victim, box);
        for (int i = 0; i < near.size(); i++) {
            Entity n = near.get(i);
            if (n == self) continue;
            if (n instanceof EntityPlayer && ((EntityPlayer) n).isSwingInProgress) {
                tg.attribution.foreignAttack(now);
            } else if (n instanceof EntityArrow && ((EntityArrow) n).shootingEntity != self
                    && n.getDistanceSqToEntity(victim) < 9.0) {
                tg.attribution.foreignAttack(now);
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
        for (Target tg : targets.values()) {
            if (!tg.fired && now - tg.attribution.lastOwnHitMs() <= CHAT_WINDOW_MS
                    && KillMessageMatcher.isKillMessage(text, self, tg.name)) {
                fire(tg, now);
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
        tg.fired = true; // the other confirmation of this kill is ignored
        Long last = cooldowns.get(tg.name);
        if (last != null && now - last < COOLDOWN_MS) return;
        cooldowns.put(tg.name, now);
        if (cooldowns.size() > 256) cooldowns.clear(); // bounded memory
        effects.trigger(tg.x, tg.y, tg.z);
    }
}
