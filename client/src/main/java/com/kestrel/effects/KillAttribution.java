package com.kestrel.effects;

/**
 * Decides whether a player's death seen by the client was <em>your</em> kill,
 * from client-visible facts only. Pure (no Minecraft types) so every rule is
 * unit-tested.
 *
 * <p>Minecraft 1.8.9 does not tell a client who killed whom (apart from the
 * server's death message, see {@link KillMessageMatcher}). What the client
 * does see, per entity:
 * <ul>
 *   <li>when <em>you</em> attack it ({@code AttackEntityEvent});</li>
 *   <li>when it takes damage from anything: the server sends a "hurt" status
 *   for every damage, including the killing blow, which starts the red hurt
 *   animation ({@code hurtTime});</li>
 *   <li>when it dies (health 0);</li>
 *   <li>other players swinging nearby and arrows flying near it.</li>
 * </ul>
 * A death is credited to you only when the damage that killed it is
 * attributable to your hit: the last hurt event arrived within
 * {@link #OWN_HIT_WINDOW_MS} after your attack (the round trip to the server),
 * the death followed your attack closely, and nobody else attacked the victim
 * in that window. Fall, lava, fire, mob or another player's damage after your
 * hit produces a hurt event that is not yours, so it never counts. When it is
 * ambiguous the answer is "not yours": a missed effect is better than a
 * wrong one.
 */
public final class KillAttribution {
    /** Longest delay between your swing and the server's hurt event for it. */
    static final long OWN_HIT_WINDOW_MS = 1000;
    /** The death must come this soon after your last hit. */
    static final long DEATH_WINDOW_MS = 1500;
    /** Another attacker this long before your hit (or after it) makes the kill ambiguous. */
    static final long FOREIGN_WINDOW_MS = 1000;

    private static final long NEVER = Long.MIN_VALUE / 4;

    private long lastOwnHitMs = NEVER;
    private long lastHurtMs = NEVER;
    private boolean lastDamageOurs;
    private long lastForeignAttackMs = NEVER;

    /** You attacked the victim. */
    public void ownHit(long nowMs) {
        lastOwnHitMs = nowMs;
    }

    /** The victim was hurt (a new hurt animation started). */
    public void hurt(long nowMs) {
        lastHurtMs = nowMs;
        lastDamageOurs = nowMs >= lastOwnHitMs && nowMs - lastOwnHitMs <= OWN_HIT_WINDOW_MS;
    }

    /** Someone else (another player's swing, a projectile not yours) attacked near the victim. */
    public void foreignAttack(long nowMs) {
        lastForeignAttackMs = nowMs;
    }

    public long lastOwnHitMs() {
        return lastOwnHitMs;
    }

    /** Whether the victim's death, observed now, is credited to you. */
    public boolean creditedDeath(long nowMs) {
        if (lastOwnHitMs == NEVER || lastHurtMs < lastOwnHitMs) return false; // no damage seen after your hit
        if (!lastDamageOurs) return false; // the last damage was not your hit
        if (nowMs - lastOwnHitMs > DEATH_WINDOW_MS) return false;
        return lastForeignAttackMs < lastOwnHitMs - FOREIGN_WINDOW_MS; // nobody else attacked around your hit
    }
}
