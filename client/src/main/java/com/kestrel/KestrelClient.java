package com.kestrel;

import com.kestrel.config.ClientConfig;
import com.kestrel.config.ConfigManager;
import com.kestrel.config.Preset;
import com.kestrel.config.PresetApplier;
import com.kestrel.rendering.Fullbright;
import com.kestrel.cosmetics.CapeManager;
import com.kestrel.cosmetics.CapeRenderer;
import com.kestrel.effects.KillDetector;
import com.kestrel.effects.KillEffectManager;
import com.kestrel.gui.ClientSettingsScreen;
import com.kestrel.gui.HudEditorScreen;
import com.kestrel.hud.CrosshairRenderer;
import com.kestrel.hud.HudManager;
import com.kestrel.camera.CameraController;
import com.kestrel.skin.MainMenuButton;
import com.kestrel.skin.SkinManager;
import com.kestrel.input.InputManager;
import com.kestrel.input.ToggleSprint;
import com.kestrel.performance.PerformanceManager;
import com.kestrel.rendering.SkyManager;
import com.kestrel.util.AsyncLoader;
import net.minecraft.client.Minecraft;
import net.minecraftforge.common.MinecraftForge;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;

import java.io.File;

/**
 * Owns every subsystem and wires them to Forge's event bus. Subsystems are
 * independent: each checks its own config switch and does no work when
 * disabled, so turning one off never affects the others.
 */
public final class KestrelClient {
    private final File dataDir;
    private final String version;
    private final ConfigManager configManager;
    private final AsyncLoader loader = new AsyncLoader();

    private HudManager hud;
    private CrosshairRenderer crosshair;
    private InputManager input;
    private CameraController camera;
    private SkinManager skins;
    private ToggleSprint toggleSprint;
    private CapeManager capes;
    private SkyManager sky;
    private KillEffectManager killEffects;
    private KillDetector killDetector;
    private PerformanceManager performance;
    private LauncherSync launcherSync;

    KestrelClient(File dataDir, String version) {
        this.dataDir = dataDir;
        this.version = version;
        this.configManager = new ConfigManager(new File(dataDir, "config.json"));
    }

    void preInit() {
        configManager.load();
        Runtime.getRuntime().addShutdownHook(new Thread(new Runnable() {
            @Override
            public void run() {
                configManager.shutdown();
                loader.shutdown();
            }
        }, "Kestrel-Shutdown"));
    }

    void init() {
        ClientConfig cfg = configManager.get();
        performance = new PerformanceManager();
        input = new InputManager(this);
        camera = new CameraController(this);
        toggleSprint = new ToggleSprint(this);
        hud = new HudManager(this); // registers HUD element defaults in the config
        crosshair = new CrosshairRenderer(this);
        capes = new CapeManager(this, new File(dataDir, "capes"));
        sky = new SkyManager(this, new File(dataDir, "skies"));
        skins = new SkinManager(this, dataDir);
        killEffects = new KillEffectManager(this);
        killDetector = new KillDetector(this, killEffects);

        // Launcher preset request (once per distinct request).
        String requested = System.getProperty("kestrel.requestedPreset");
        Preset applied = PresetApplier.applyLauncherRequest(cfg, requested);
        if (applied != null) {
            KestrelMod.LOG.info("[Kestrel] applied launcher preset {}", applied.id);
            if (cfg.performance.applyVanillaSettings) {
                performance.applyVanillaSettings(applied);
            }
        }
        // Cosmetics chosen in the launcher (applied once per revision).
        launcherSync = new LauncherSync(this, dataDir);
        launcherSync.applyNow();
        cfg.validate();
        configManager.markDirty();
        applyQol(cfg);
        skins.loadNow(); // local skin chosen in the launcher or in game

        input.registerKeybinds();
        camera.registerKeybinds();
        ActivityReporter activity = new ActivityReporter(System.getProperty("kestrel.activityToken"));
        Object[] listeners = {this, performance, input, camera, toggleSprint, hud, crosshair, capes, sky, killEffects, killDetector, launcherSync, activity, skins, new MainMenuButton(this, skins)};
        for (Object l : listeners) {
            MinecraftForge.EVENT_BUS.register(l);
        }
    }

    void postInit() {
        capes.registerRenderer(new CapeRenderer(this));
        capes.reload();
        sky.reload();
    }

    @SubscribeEvent
    public void onClientTick(TickEvent.ClientTickEvent event) {
        if (event.phase == TickEvent.Phase.END) {
            configManager.tick();
            loader.drainOnGameThread();
        }
    }

    /** Apply a preset chosen in-game. */
    public void applyPreset(Preset p) {
        ClientConfig cfg = config();
        PresetApplier.apply(cfg, p);
        if (cfg.performance.applyVanillaSettings) {
            performance.applyVanillaSettings(p);
        }
        onConfigChanged();
    }

    /** Re-apply anything derived from config and schedule a save. */
    public void onConfigChanged() {
        applyQol(config());
        sky.apply();
        capes.ensureLoaded();
        hud.invalidate();
        configManager.markDirty();
    }

    /** Push QoL settings into the render hooks (cheap; safe to call often). */
    private void applyQol(ClientConfig cfg) {
        boolean on = cfg.qol.fullbright;
        if (Fullbright.isEnabled() != on) {
            Fullbright.setEnabled(on);
            KestrelMod.LOG.info("[Kestrel] fullbright {}", on ? "on" : "off");
        }
    }

    public void openSettings() {
        Minecraft.getMinecraft().displayGuiScreen(new ClientSettingsScreen(this));
    }

    public void openHudEditor() {
        Minecraft.getMinecraft().displayGuiScreen(new HudEditorScreen(this));
    }

    public ClientConfig config() {
        return configManager.get();
    }

    public ConfigManager configManager() {
        return configManager;
    }

    public AsyncLoader loader() {
        return loader;
    }

    public File dataDir() {
        return dataDir;
    }

    public String version() {
        return version;
    }

    public HudManager hud() {
        return hud;
    }

    public InputManager input() {
        return input;
    }

    public SkinManager skins() {
        return skins;
    }

    public CameraController camera() {
        return camera;
    }

    public ToggleSprint toggleSprint() {
        return toggleSprint;
    }

    public CapeManager capes() {
        return capes;
    }

    public SkyManager sky() {
        return sky;
    }

    public KillEffectManager killEffects() {
        return killEffects;
    }

    public PerformanceManager performance() {
        return performance;
    }
}
