package com.kestrel.effects;

import org.junit.Test;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class KillMessageMatcherTest {
    @Test
    public void matchesCommonKillFeeds() {
        assertTrue(KillMessageMatcher.isKillMessage("Steve was killed by Kellen.", "Kellen", "Steve"));
        assertTrue(KillMessageMatcher.isKillMessage("§cSteve §7was slain by §aKellen§7. §b§lFINAL KILL!", "Kellen", "Steve"));
        assertTrue(KillMessageMatcher.isKillMessage("Kellen killed Steve!", "Kellen", "Steve"));
    }

    @Test
    public void rejectsOtherKillsAndChat() {
        assertFalse(KillMessageMatcher.isKillMessage("Steve was killed by Alex.", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("<Steve> Kellen is good", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("[MVP+] Steve: gg Kellen", "Kellen", "Steve"));
        // Whole-word matching: names that merely contain the target do not count.
        assertFalse(KillMessageMatcher.isKillMessage("Steve2 was killed by Kellen.", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("Steve was killed by Kellen_X.", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("x", "Kellen", "Kellen"));
        assertFalse(KillMessageMatcher.isKillMessage(null, "Kellen", "Steve"));
    }
}
