package com.kestrel;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNull;

public class ActivityReporterTest {
    private static final String TOKEN = "0123456789abcdef0123456789abcdef";

    @Test
    public void statesFollowWorldAndIntegratedServer() {
        assertEquals(ActivityReporter.State.MENU, ActivityReporter.stateOf(false, false));
        assertEquals(ActivityReporter.State.MENU, ActivityReporter.stateOf(false, true));
        assertEquals(ActivityReporter.State.SINGLEPLAYER, ActivityReporter.stateOf(true, true));
        assertEquals(ActivityReporter.State.MULTIPLAYER, ActivityReporter.stateOf(true, false));
    }

    /** Must match kestrel-core's launch::parse_activity. */
    @Test
    public void lineFormatMatchesTheLauncher() {
        assertEquals("[Kestrel] activity " + TOKEN + ": multiplayer",
                new ActivityReporter(TOKEN).line(ActivityReporter.State.MULTIPLAYER));
        assertEquals("[Kestrel] activity " + TOKEN + ": menu",
                new ActivityReporter(TOKEN).line(ActivityReporter.State.MENU));
    }

    @Test
    public void nothingIsReportedWithoutAValidToken() {
        assertNull(new ActivityReporter(null).line(ActivityReporter.State.MENU));
        assertNull(new ActivityReporter("").line(ActivityReporter.State.MENU));
        assertNull(new ActivityReporter("not-hex; injected").line(ActivityReporter.State.MENU));
    }
}
