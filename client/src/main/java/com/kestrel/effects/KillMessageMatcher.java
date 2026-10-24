package com.kestrel.effects;

/**
 * Heuristic chat kill-message matching (pure, unit-tested).
 *
 * <p>There is no universal kill event for 1.8.9 multiplayer: servers phrase
 * kill messages differently ("X was killed by Y", "X was slain by Y",
 * "Y killed X", localised variants…). We only claim a kill when a chat line
 * contains <em>both</em> the victim's name (someone we hit recently) and our
 * own name as whole words. This avoids false positives from other players'
 * kills, at the cost of missing servers that omit the killer's name.
 */
public final class KillMessageMatcher {
    private KillMessageMatcher() {
    }

    /** Strip legacy § colour codes. */
    public static String stripFormatting(String s) {
        StringBuilder b = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '§' && i + 1 < s.length()) {
                i++;
                continue;
            }
            b.append(c);
        }
        return b.toString();
    }

    public static boolean isKillMessage(String message, String self, String victim) {
        if (message == null || self == null || victim == null || self.isEmpty() || victim.isEmpty()
                || self.equalsIgnoreCase(victim)) {
            return false;
        }
        String m = stripFormatting(message);
        // Chat lines from players look like "<Name> text" or "Name: text"; a
        // player typing both names must not trigger an effect.
        if (looksLikePlayerChat(m)) {
            return false;
        }
        return containsWord(m, victim) && containsWord(m, self);
    }

    static boolean looksLikePlayerChat(String m) {
        String t = m.trim();
        if (t.startsWith("<") && t.indexOf('>') > 1) return true;
        int colon = t.indexOf(": ");
        // "[RANK] Name: message" style. Kill feeds rarely contain ": ".
        return colon > 0 && colon < 40;
    }

    /** Case-sensitive whole-word match ([A-Za-z0-9_] are word characters). */
    static boolean containsWord(String haystack, String word) {
        int from = 0;
        while (true) {
            int i = haystack.indexOf(word, from);
            if (i < 0) return false;
            int end = i + word.length();
            boolean leftOk = i == 0 || !isNameChar(haystack.charAt(i - 1));
            boolean rightOk = end == haystack.length() || !isNameChar(haystack.charAt(end));
            if (leftOk && rightOk) return true;
            from = i + 1;
        }
    }

    private static boolean isNameChar(char c) {
        return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_';
    }
}
