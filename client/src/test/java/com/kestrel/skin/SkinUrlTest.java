package com.kestrel.skin;

import org.junit.Test;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Base64;
import java.util.List;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

public class SkinUrlTest {
    private static void ok(String in, SkinUrl.Kind kind, String value) throws Exception {
        SkinUrl.Source s = SkinUrl.parse(in);
        assertEquals(in, kind, s.kind);
        assertEquals(in, value, s.value);
    }

    @Test
    public void parsesSupportedLinks() throws Exception {
        ok("https://namemc.com/profile/Notch", SkinUrl.Kind.PROFILE, "Notch");
        ok(" namemc.com/profile/Notch.1 ", SkinUrl.Kind.PROFILE, "Notch");
        ok("https://de.namemc.com/profile/jeb_?x=1#a", SkinUrl.Kind.PROFILE, "jeb_");
        ok("https://namemc.com/profile/069a79f4-44e9-4726-a5be-fca90e38aaf5", SkinUrl.Kind.PROFILE, "069a79f444e94726a5befca90e38aaf5");
        ok("https://namemc.com/skin/1b0bb2e4a3d2d3a0", SkinUrl.Kind.NAMEMC_SKIN, "1b0bb2e4a3d2d3a0");
        ok("http://textures.minecraft.net/texture/abc123", SkinUrl.Kind.TEXTURE, "https://textures.minecraft.net/texture/abc123");
    }

    @Test
    public void rejectsEverythingElse() {
        StringBuilder longName = new StringBuilder("https://namemc.com/profile/");
        for (int i = 0; i < 400; i++) longName.append('a');
        String[] bad = {"", "https://evil.com/profile/Notch", "https://namemc.com.evil.com/profile/Notch",
                "https://evilnamemc.com/profile/Notch", "https://user@namemc.com/profile/Notch",
                "https://namemc.com:8443/profile/Notch", "ftp://namemc.com/profile/Notch", "file:///C:/skin.png",
                "https://namemc.com/profile/a", "https://namemc.com/profile/bad%20name", "https://namemc.com/server/hypixel.net",
                "https://textures.minecraft.net/texture/../../x", "https://abc.namemc.com/profile/Notch", longName.toString()};
        for (String b : bad) {
            try {
                SkinUrl.parse(b);
                fail("accepted " + b);
            } catch (SkinStore.SkinException expected) {
                assertTrue(!expected.getMessage().isEmpty());
            }
        }
    }

    static byte[] profile(String url, boolean slim) {
        String textures = url == null ? "{\"textures\":{}}"
                : "{\"textures\":{\"SKIN\":{\"url\":\"" + url + "\"" + (slim ? ",\"metadata\":{\"model\":\"slim\"}" : "") + "}}}";
        String b64 = Base64.getEncoder().encodeToString(textures.getBytes(StandardCharsets.UTF_8));
        return ("{\"id\":\"069a79f444e94726a5befca90e38aaf5\",\"name\":\"Notch\",\"properties\":[{\"name\":\"textures\",\"value\":\"" + b64 + "\"}]}")
                .getBytes(StandardCharsets.UTF_8);
    }

    @Test
    public void resolvesProfilesThroughMojang() throws Exception {
        final byte[] skin = SkinStoreTest.png(64, 64, true);
        final List<String> calls = new ArrayList<String>();
        SkinUrl.Fetcher f = new SkinUrl.Fetcher() {
            @Override
            public byte[] fetch(String url, int limit) throws IOException {
                calls.add(url);
                if (url.equals("https://api.mojang.com/users/profiles/minecraft/notch")) {
                    return "{\"id\":\"069a79f444e94726a5befca90e38aaf5\",\"name\":\"Notch\"}".getBytes(StandardCharsets.UTF_8);
                }
                if (url.equals("https://sessionserver.mojang.com/session/minecraft/profile/069a79f444e94726a5befca90e38aaf5")) {
                    return profile("http://textures.minecraft.net/texture/deadbeef", true);
                }
                if (url.equals("https://textures.minecraft.net/texture/deadbeef")) return skin;
                throw new IOException("unexpected " + url);
            }
        };
        SkinUrl.Downloaded d = SkinUrl.resolve(SkinUrl.parse("namemc.com/profile/notch"), f);
        assertEquals("Notch", d.name);
        assertEquals(SkinStore.MODEL_SLIM, d.model);
        assertArrayEquals(skin, d.bytes);
        assertEquals(3, calls.size());
    }

    @Test
    public void explainsFailures() throws Exception {
        SkinUrl.Fetcher f = new SkinUrl.Fetcher() {
            @Override
            public byte[] fetch(String url, int limit) throws IOException {
                if (url.contains("api.mojang.com")) throw new IOException("HTTP 404 from api.mojang.com");
                return profile(null, false);
            }
        };
        expect(SkinUrl.parse("namemc.com/profile/nobody_here"), f, "no Minecraft account");
        expect(SkinUrl.parse("namemc.com/profile/069a79f444e94726a5befca90e38aaf5"), f, "default skin");
        expect(SkinUrl.parse("namemc.com/skin/abc"), f, "Download");
        try {
            SkinUrl.parseProfileTextures(profile("https://evil.com/x.png", false));
            fail();
        } catch (SkinStore.SkinException expected) {
            assertTrue(expected.getMessage().contains("unexpected"));
        }
    }

    private static void expect(SkinUrl.Source s, SkinUrl.Fetcher f, String text) throws IOException {
        try {
            SkinUrl.resolve(s, f);
            fail();
        } catch (SkinStore.SkinException e) {
            assertTrue(e.getMessage(), e.getMessage().contains(text));
        }
    }

    @Test
    public void realFetcherRefusesOtherHostsWithoutNetwork() {
        for (String url : new String[] {"https://namemc.com/x", "http://textures.minecraft.net/texture/a", "https://textures.minecraft.net.evil.com/x"}) {
            try {
                SkinUrl.HTTP.fetch(url, 10);
                fail(url);
            } catch (IOException e) {
                assertTrue(e.getMessage().contains("refusing"));
            }
        }
    }
}
