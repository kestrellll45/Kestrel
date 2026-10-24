package com.kestrel.util;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;

import java.util.concurrent.Callable;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ThreadFactory;

/**
 * One background thread for disk I/O and image decoding, plus a queue of
 * completions that run on the game thread (where OpenGL calls are legal).
 * Never blocks the render thread and never creates a thread per task.
 */
public final class AsyncLoader {
    private static final Logger LOG = LogManager.getLogger("Kestrel");
    /** Completions processed per tick, to keep frame times flat. */
    private static final int MAX_COMPLETIONS_PER_TICK = 4;

    public interface Callback<T> {
        /** Runs on the game thread. {@code error} is null on success. */
        void done(T result, Throwable error);
    }

    private final ExecutorService worker = Executors.newSingleThreadExecutor(new ThreadFactory() {
        @Override
        public Thread newThread(Runnable r) {
            Thread t = new Thread(r, "Kestrel-Loader");
            t.setDaemon(true);
            t.setPriority(Thread.NORM_PRIORITY - 1);
            return t;
        }
    });
    private final ConcurrentLinkedQueue<Runnable> completions = new ConcurrentLinkedQueue<Runnable>();

    public <T> void submit(final Callable<T> task, final Callback<T> callback) {
        try {
            worker.execute(new Runnable() {
                @Override
                public void run() {
                    T result = null;
                    Throwable error = null;
                    try {
                        result = task.call();
                    } catch (Throwable t) {
                        error = t;
                    }
                    final T r = result;
                    final Throwable e = error;
                    completions.add(new Runnable() {
                        @Override
                        public void run() {
                            callback.done(r, e);
                        }
                    });
                }
            });
        } catch (RejectedExecutionException e) {
            LOG.warn("[Kestrel] loader is shut down; task dropped");
        }
    }

    /** Called once per client tick on the game thread. */
    public void drainOnGameThread() {
        for (int i = 0; i < MAX_COMPLETIONS_PER_TICK; i++) {
            Runnable r = completions.poll();
            if (r == null) return;
            try {
                r.run();
            } catch (RuntimeException e) {
                LOG.error("[Kestrel] async completion failed", e);
            }
        }
    }

    public void shutdown() {
        worker.shutdownNow();
    }
}
