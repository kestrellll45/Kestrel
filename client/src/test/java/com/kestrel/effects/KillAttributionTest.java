package com.kestrel.effects;

import org.junit.Test;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

/** Every case from the kill-effect spec, as client-visible event sequences. */
public class KillAttributionTest {
    @Test
    public void yourKillingBlowCounts() {
        KillAttribution a = new KillAttribution();
        a.ownHit(1000);
        a.hurt(1080); // server confirms your hit (80 ms ping)
        assertTrue(a.creditedDeath(1080)); // ...and the victim died from it
    }

    @Test
    public void highPingStillCounts() {
        KillAttribution a = new KillAttribution();
        a.ownHit(1000);
        a.hurt(1900);
        assertTrue(a.creditedDeath(1950));
    }

    @Test
    public void environmentalDeathAfterYourHitDoesNotCount() {
        // You hit, then the victim later burns, falls or drowns to death.
        KillAttribution a = new KillAttribution();
        a.ownHit(1000);
        a.hurt(1080);
        a.hurt(2600); // lava / fall / fire damage, not your hit
        assertFalse(a.creditedDeath(2600));
    }

    @Test
    public void anotherPlayersFinishDoesNotCount() {
        KillAttribution a = new KillAttribution();
        a.ownHit(1000);
        a.hurt(1080);
        a.foreignAttack(1300); // someone else swings at the victim
        a.hurt(1350);
        assertFalse(a.creditedDeath(1350));
    }

    @Test
    public void simultaneousAttackerMakesItAmbiguous() {
        // Another player swung just before your hit: the server may credit them.
        KillAttribution a = new KillAttribution();
        a.foreignAttack(900);
        a.ownHit(1000);
        a.hurt(1080);
        assertFalse(a.creditedDeath(1080));
        // ...but an old foreign attack (well before your hit) does not matter.
        KillAttribution b = new KillAttribution();
        b.foreignAttack(-5000);
        b.ownHit(1000);
        b.hurt(1080);
        assertTrue(b.creditedDeath(1080));
    }

    @Test
    public void deathWithoutYourHitNeverCounts() {
        KillAttribution a = new KillAttribution();
        a.hurt(1000); // died to a mob / the void; you never hit it
        assertFalse(a.creditedDeath(1000));
    }

    @Test
    public void noDamageSeenAfterYourHitDoesNotCount() {
        KillAttribution a = new KillAttribution();
        a.hurt(500); // earlier damage
        a.ownHit(1000); // your swing missed or was blocked: no hurt event follows
        assertFalse(a.creditedDeath(1200));
    }

    @Test
    public void lateDeathDoesNotCount() {
        KillAttribution a = new KillAttribution();
        a.ownHit(1000);
        a.hurt(1080);
        assertFalse(a.creditedDeath(1000 + KillAttribution.DEATH_WINDOW_MS + 1));
    }
}
