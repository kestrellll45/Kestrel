package com.kestrel.effects;

import java.util.Locale;

/**
 * Kill-feed message matching (pure, unit-tested).
 *
 * <p>There is no universal kill event for 1.8.9 multiplayer: servers phrase
 * kill messages differently ("X was slain by Y", "X was knocked into the
 * void by Y", "Y killed X", ...). A line counts as <em>your</em> kill of the
 * victim only when it names both of you as whole words <em>and</em> its
 * wording makes you the killer:
 * <ul>
 *   <li>victim first, then "... by &lt;you&gt;" ("Steve was slain by Kellen");</li>
 *   <li>you first, then an active kill verb and the victim ("Kellen killed Steve").</li>
 * </ul>
 * "You were slain by the victim", assists ("... assisted by you"), other
 * players' kills and player chat never match. Lines that do not fit either
 * shape are ignored: a missed effect is better than a wrong one.
 */
public final class KillMessageMatcher {
    /** Verbs a kill feed uses for "killer &lt;verb&gt; victim". */
    private static final String[] ACTIVE_VERBS = {
            "killed", "slew", "slain", "eliminated", "destroyed", "finished", "defeated", "shot", "murdered",
            "executed", "obliterated", "knocked", "pushed", "blew up", "exploded", "rekt", "pwned", "took out",
            "struck down", "has killed", "final killed",
    };

    private KillMessageMatcher() {
    }

    /** Strip legacy section-sign colour codes. */
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
        int vi = wordIndex(m, victim);
        int si = wordIndex(m, self);
        if (vi < 0 || si < 0) return false;
        if (vi < si) {
            // "<victim> ... by <you>": the words right before your name must be "by",
            // and not "assisted by" / "assist by".
            String before = m.substring(vi + victim.length(), si).toLowerCase(Locale.ROOT).trim();
            if (!endsWithWord(before, "by")) return false;
            String head = before.substring(0, before.length() - 2).trim();
            return !(endsWithWord(head, "assisted") || endsWithWord(head, "assist") || endsWithWord(head, "and"));
        }
        // "<you> ... <victim>": active voice only ("you killed victim"), never
        // "you were killed by victim".
        String between = " " + m.substring(si + self.length(), vi).toLowerCase(Locale.ROOT).trim() + " ";
        if (between.contains(" by ") || between.contains(" was ") || between.contains(" were ")
                || between.contains(" died") || between.contains(" assist")) {
            return false;
        }
        for (String verb : ACTIVE_VERBS) {
            if (between.contains(" " + verb + " ") || between.trim().equals(verb)) return true;
        }
        return false;
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
        return wordIndex(haystack, word) >= 0;
    }

    /** Index of the first whole-word occurrence, or -1. */
    static int wordIndex(String haystack, String word) {
        int from = 0;
        while (true) {
            int i = haystack.indexOf(word, from);
            if (i < 0) return -1;
            int end = i + word.length();
            boolean leftOk = i == 0 || !isNameChar(haystack.charAt(i - 1));
            boolean rightOk = end == haystack.length() || !isNameChar(haystack.charAt(end));
            if (leftOk && rightOk) return i;
            from = i + 1;
        }
    }

    private static boolean endsWithWord(String s, String word) {
        if (!s.endsWith(word)) return false;
        int i = s.length() - word.length();
        return i == 0 || !isNameChar(s.charAt(i - 1));
    }

    private static boolean isNameChar(char c) {
        return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_';
    }
}
