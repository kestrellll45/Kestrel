package com.kestrel.config;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;

import java.io.File;
import java.io.IOException;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ThreadFactory;
import java.util.concurrent.TimeUnit;

/**
 * Loads, validates, migrates and saves {@link ClientConfig}.
 *
 * <p>Threading: the config object is only read and mutated on the game
 * thread. Saving is debounced: callers mark it dirty, {@link #tick()} takes a
 * JSON snapshot on the game thread at most once per {@link #SAVE_DELAY_MS},
 * and a single background thread writes it. Writes are atomic (temp file +
 * rename) so a crash mid-write cannot corrupt the config.
 */
public final class ConfigManager {
    private static final Logger LOG = LogManager.getLogger("Kestrel");
    static final long SAVE_DELAY_MS = 1000;

    private final File file;
    private final Gson gson = new GsonBuilder().setPrettyPrinting().create();
    private final ExecutorService io = Executors.newSingleThreadExecutor(new ThreadFactory() {
        @Override
        public Thread newThread(Runnable r) {
            Thread t = new Thread(r, "Kestrel-Config-IO");
            t.setDaemon(true);
            return t;
        }
    });
    /** Serialises file writes between the IO thread and shutdown. */
    private final Object writeLock = new Object();
    private ClientConfig config;
    // Game-thread only.
    private boolean dirty;
    private long dirtySinceMs;

    public ConfigManager(File file) {
        this.file = file;
    }

    public ClientConfig get() {
        return config;
    }

    /** Load from disk; never throws. Malformed files are preserved as .bad. */
    public ClientConfig load() {
        config = readOrDefault(file, gson);
        return config;
    }

    static ClientConfig readOrDefault(File file, Gson gson) {
        if (!file.isFile()) {
            ClientConfig c = new ClientConfig();
            c.validate();
            return c;
        }
        try {
            String text = new String(Files.readAllBytes(file.toPath()), StandardCharsets.UTF_8);
            JsonElement tree = new JsonParser().parse(text);
            if (!tree.isJsonObject()) {
                throw new JsonParseException("root is not an object");
            }
            JsonObject obj = migrate(tree.getAsJsonObject());
            ClientConfig c = gson.fromJson(obj, ClientConfig.class);
            if (c == null) {
                c = new ClientConfig();
            }
            c.configVersion = ClientConfig.CURRENT_VERSION;
            c.validate();
            return c;
        } catch (IOException | RuntimeException e) {
            // RuntimeException covers Gson's JsonParseException/JsonSyntaxException
            // and IllegalStateException for wrong value types.
            File bad = new File(file.getParentFile(), file.getName() + ".bad");
            try {
                Files.copy(file.toPath(), bad.toPath(), StandardCopyOption.REPLACE_EXISTING);
            } catch (IOException copyErr) {
                LOG.error("[Kestrel] could not back up malformed config", copyErr);
            }
            LOG.error("[Kestrel] config {} is malformed ({}); using defaults, original kept as {}",
                    file, e.toString(), bad.getName());
            ClientConfig c = new ClientConfig();
            c.validate();
            return c;
        }
    }

    /** Upgrade older config layouts in-place. */
    static JsonObject migrate(JsonObject obj) {
        int version = obj.has("configVersion") && obj.get("configVersion").isJsonPrimitive()
                ? obj.get("configVersion").getAsInt() : 0;
        if (version > ClientConfig.CURRENT_VERSION) {
            LOG.warn("[Kestrel] config is from a newer client (v{}); unknown settings are ignored", version);
        }
        // v0 -> v1: no structural changes (v1 is the first released schema).
        obj.addProperty("configVersion", ClientConfig.CURRENT_VERSION);
        return obj;
    }

    /**
     * Request a save. Must be called on the game thread (all config mutation
     * happens there). Cheap; safe to call every frame.
     */
    public void markDirty() {
        if (!dirty) {
            dirty = true;
            dirtySinceMs = System.currentTimeMillis();
        }
    }

    /**
     * Called every client tick on the game thread. Once the config has been
     * dirty for {@link #SAVE_DELAY_MS}, a JSON snapshot is taken here (so no
     * other thread ever reads the live config) and written in the background.
     */
    public void tick() {
        if (dirty && System.currentTimeMillis() - dirtySinceMs >= SAVE_DELAY_MS) {
            final String json = snapshot();
            try {
                io.execute(new Runnable() {
                    @Override
                    public void run() {
                        write(json);
                    }
                });
            } catch (java.util.concurrent.RejectedExecutionException shuttingDown) {
                write(json);
            }
        }
    }

    private String snapshot() {
        dirty = false;
        dirtySinceMs = 0;
        return gson.toJson(config);
    }

    private void write(String json) {
        synchronized (writeLock) {
            try {
                writeAtomically(file, json);
            } catch (IOException e) {
                LOG.error("[Kestrel] failed to save config", e);
            }
        }
    }

    /**
     * Write immediately on the calling thread. Used at shutdown, when the game
     * loop has stopped mutating the config.
     */
    public void saveNow() {
        if (config != null) {
            write(snapshot());
        }
    }

    /** Stop the IO thread and write any pending changes (called on shutdown). */
    public void shutdown() {
        io.shutdown();
        try {
            io.awaitTermination(3, TimeUnit.SECONDS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        if (dirty) {
            saveNow();
        }
    }

    static void writeAtomically(File target, String json) throws IOException {
        File dir = target.getAbsoluteFile().getParentFile();
        if (dir != null && !dir.isDirectory() && !dir.mkdirs()) {
            throw new IOException("cannot create " + dir);
        }
        File tmp = new File(dir, target.getName() + ".tmp");
        Writer w = new OutputStreamWriter(Files.newOutputStream(tmp.toPath()), StandardCharsets.UTF_8);
        try {
            w.write(json);
        } finally {
            w.close();
        }
        try {
            Files.move(tmp.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
        } catch (AtomicMoveNotSupportedException e) {
            Files.move(tmp.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING);
        }
    }
}
