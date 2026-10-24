package com.kestrel;

import net.minecraft.client.Minecraft;
import net.minecraftforge.fml.common.Mod;
import net.minecraftforge.fml.common.event.FMLInitializationEvent;
import net.minecraftforge.fml.common.event.FMLPostInitializationEvent;
import net.minecraftforge.fml.common.event.FMLPreInitializationEvent;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;

import java.io.File;

/**
 * Forge entry point. Client-only and accepted by any server (vanilla servers
 * such as Hypixel never see this mod).
 */
@Mod(modid = KestrelMod.MODID, useMetadata = true, clientSideOnly = true,
        acceptedMinecraftVersions = "[1.8.9]", acceptableRemoteVersions = "*")
public class KestrelMod {
    public static final String MODID = "kestrel";
    public static final Logger LOG = LogManager.getLogger("Kestrel");

    private static KestrelClient client;

    public static KestrelClient client() {
        return client;
    }

    @Mod.EventHandler
    public void preInit(FMLPreInitializationEvent event) {
        File dataDir = new File(Minecraft.getMinecraft().mcDataDir, "kestrel");
        if (!dataDir.isDirectory() && !dataDir.mkdirs()) {
            LOG.error("[Kestrel] cannot create data directory {}", dataDir);
        }
        client = new KestrelClient(dataDir, event.getModMetadata().version);
        client.preInit();
    }

    @Mod.EventHandler
    public void init(FMLInitializationEvent event) {
        client.init();
    }

    @Mod.EventHandler
    public void postInit(FMLPostInitializationEvent event) {
        client.postInit();
        // The launcher watches stdout for this exact marker to confirm the
        // client mod loaded. Keep in sync with launch.rs MOD_READY_MARKER.
        LOG.info("[Kestrel] Client initialized (version {})", client.version());
    }
}
