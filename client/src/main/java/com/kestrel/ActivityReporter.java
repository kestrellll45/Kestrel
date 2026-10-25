package com.kestrel;

import net.minecraft.client.Minecraft;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;

/**
 * Tells the launcher what the player is doing, for Discord Rich Presence.
 *
 * <p>On each change it logs {@code [Kestrel] activity <token>: <state>} where
 * the state is {@code menu}, {@code singleplayer} or {@code multiplayer} and
 * the token is the per-launch value the launcher passes as
 * {@code -Dkestrel.activityToken} (so chat cannot spoof it). Nothing else is
 * reported: no server address, world name or player data. Without a token
 * (e.g. {@code gradlew runClient}) nothing is logged.
 */
public final class ActivityReporter {
    public static final String MARKER = "[Kestrel] activity ";

    public enum State {
        MENU("menu"), SINGLEPLAYER("singleplayer"), MULTIPLAYER("multiplayer");

        public final String id;

        State(String id) {
            this.id = id;
        }
    }

    private final String token;
    private State last;

    ActivityReporter(String token) {
        this.token = token == null || !token.matches("[0-9a-f]{32}") ? null : token;
    }

    /** Pure: the state for a client with/without a world and integrated server. */
    public static State stateOf(boolean inWorld, boolean integratedServer) {
        if (!inWorld) return State.MENU;
        return integratedServer ? State.SINGLEPLAYER : State.MULTIPLAYER;
    }

    /** The log line for a state, or null when no token was given. */
    String line(State s) {
        return token == null ? null : MARKER + token + ": " + s.id;
    }

    @SubscribeEvent
    public void onTick(TickEvent.ClientTickEvent event) {
        if (event.phase != TickEvent.Phase.END || token == null) return;
        Minecraft mc = Minecraft.getMinecraft();
        State now = stateOf(mc.theWorld != null, mc.isSingleplayer());
        if (now != last) {
            last = now;
            KestrelMod.LOG.info(line(now));
        }
    }
}
