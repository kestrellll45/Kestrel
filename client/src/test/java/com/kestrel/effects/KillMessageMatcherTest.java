package com.kestrel.effects;

import org.junit.Test;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class KillMessageMatcherTest {
    @Test
    public void matchesYourKills() {
        assertTrue(KillMessageMatcher.isKillMessage("Steve was killed by Kellen.", "Kellen", "Steve"));
        assertTrue(KillMessageMatcher.isKillMessage("Steve was slain by Kellen", "Kellen", "Steve"));
        assertTrue(KillMessageMatcher.isKillMessage("Steve was slain by Kellen using [Diamond Sword]", "Kellen", "Steve"));
        assertTrue(KillMessageMatcher.isKillMessage("§cSteve §7was slain by §aKellen§7. §b§lFINAL KILL!", "Kellen", "Steve"));
        assertTrue(KillMessageMatcher.isKillMessage("Steve was knocked into the void by Kellen.", "Kellen", "Steve"));
        assertTrue(KillMessageMatcher.isKillMessage("Steve was shot by Kellen", "Kellen", "Steve"));
        assertTrue(KillMessageMatcher.isKillMessage("Kellen killed Steve!", "Kellen", "Steve"));
        assertTrue(KillMessageMatcher.isKillMessage("Kellen has killed Steve", "Kellen", "Steve"));
        assertTrue(KillMessageMatcher.isKillMessage("Kellen eliminated Steve", "Kellen", "Steve"));
    }

    @Test
    public void yourOwnDeathNeverMatches() {
        assertFalse(KillMessageMatcher.isKillMessage("Kellen was slain by Steve", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("Kellen was knocked into the void by Steve.", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("Steve killed Kellen!", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("Kellen died", "Kellen", "Steve"));
    }

    @Test
    public void otherKillsAssistsAndEnvironmentDoNotMatch() {
        assertFalse(KillMessageMatcher.isKillMessage("Steve was killed by Alex.", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("Steve was killed by Alex, assisted by Kellen", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("Steve was slain by Alex and Kellen", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("Steve fell from a high place", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("Steve tried to swim in lava to escape Kellen", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("Kellen and Steve joined the game", "Kellen", "Steve"));
    }

    @Test
    public void rejectsChatAndPartialNames() {
        assertFalse(KillMessageMatcher.isKillMessage("<Steve> Kellen killed me lol", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("[MVP+] Steve: gg Kellen", "Kellen", "Steve"));
        // Whole-word matching: names that merely contain the target do not count.
        assertFalse(KillMessageMatcher.isKillMessage("Steve2 was killed by Kellen.", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("Steve was killed by Kellen_X.", "Kellen", "Steve"));
        assertFalse(KillMessageMatcher.isKillMessage("x", "Kellen", "Kellen"));
        assertFalse(KillMessageMatcher.isKillMessage(null, "Kellen", "Steve"));
    }
}
