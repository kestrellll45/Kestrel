package com.kestrel;

import com.kestrel.config.CosmeticsRequest;
import com.kestrel.util.AsyncLoader;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.concurrent.Callable;

/**
 * Picks up cosmetics chosen in the launcher's Cosmetics page.
 *
 * <p>Checked once at start-up (on the game thread, before anything renders)
 * and then once per second while the game runs. The periodic check only
 * stats the file on the loader thread; the file is read only when its
 * timestamp or size changes, and the result is applied on the game thread.
 */
public final class LauncherSync {
    static final String FILE_NAME = "launcher-cosmetics.json";
    private static final int CHECK_INTERVAL_TICKS = 20;
    private static final long MAX_BYTES = 64 * 1024;

    private final KestrelClient client;
    private final File file;
    private int ticks;
    private boolean inFlight;
    private long seenModified = -1;
    private long seenLength = -1;

    LauncherSync(KestrelClient client, File dataDir) {
        this.client = client;
        this.file = new File(dataDir, FILE_NAME);
    }

    /** Synchronous check used during start-up. Returns true if applied. */
    boolean applyNow() {
        String json = readIfChanged();
        return json != null && apply(json);
    }

    @SubscribeEvent
    public void onTick(TickEvent.ClientTickEvent event) {
        if (event.phase != TickEvent.Phase.END || inFlight || ++ticks < CHECK_INTERVAL_TICKS) return;
        ticks = 0;
        inFlight = true;
        client.loader().submit(new Callable<String>() {
            @Override
            public String call() {
                return readIfChanged();
            }
        }, new AsyncLoader.Callback<String>() {
            @Override
            public void done(String json, Throwable error) {
                inFlight = false;
                if (error != null) {
                    KestrelMod.LOG.warn("[Kestrel] cannot read {}: {}", FILE_NAME, error.toString());
                } else if (json != null && apply(json)) {
                    client.onConfigChanged();
                    client.capes().reload(); // pick up newly uploaded files
                    client.sky().reload();
                }
            }
        });
    }

    /** Returns the file contents if it changed since the last read, else null. */
    private String readIfChanged() {
        long modified = file.lastModified();
        long length = file.length();
        if (modified == 0L || (modified == seenModified && length == seenLength)) return null;
        seenModified = modified;
        seenLength = length;
        if (length > MAX_BYTES) {
            KestrelMod.LOG.warn("[Kestrel] ignoring oversized {}", FILE_NAME);
            return null;
        }
        try {
            return new String(Files.readAllBytes(file.toPath()), StandardCharsets.UTF_8);
        } catch (java.io.IOException e) {
            // Possibly mid-replace by the launcher; retry next time.
            seenModified = -1;
            return null;
        }
    }

    /** Game thread only. */
    private boolean apply(String json) {
        try {
            if (CosmeticsRequest.apply(client.config(), json)) {
                client.configManager().markDirty();
                KestrelMod.LOG.info("[Kestrel] applied cosmetics from launcher (revision {})",
                        client.config().lastLauncherCosmeticsRevision);
                return true;
            }
        } catch (RuntimeException e) {
            KestrelMod.LOG.warn("[Kestrel] invalid {}: {}", FILE_NAME, e.toString());
        }
        return false;
    }
}
