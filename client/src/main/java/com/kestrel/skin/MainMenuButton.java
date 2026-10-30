package com.kestrel.skin;

import com.kestrel.KestrelClient;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiButton;
import net.minecraft.client.gui.GuiMainMenu;
import net.minecraftforge.client.event.GuiScreenEvent;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

/**
 * Adds a vanilla-style "Skin" button to the main menu, to the right of
 * Quit Game (the language button sits on the left of that row), opening
 * {@link SkinScreen}.
 */
public final class MainMenuButton {
    /** Unlikely to clash with vanilla (0-14) or other mods' ids. */
    static final int BUTTON_ID = 0x4B5301;
    private static final int OPTIONS_BUTTON_ID = 0;

    private final KestrelClient client;
    private final SkinManager skins;

    public MainMenuButton(KestrelClient client, SkinManager skins) {
        this.client = client;
        this.skins = skins;
    }

    @SubscribeEvent
    public void onInit(GuiScreenEvent.InitGuiEvent.Post event) {
        if (!(event.gui instanceof GuiMainMenu)) return;
        // Align with the Options / Quit Game row wherever it is.
        int y = event.gui.height / 4 + 48 + 84;
        for (GuiButton b : event.buttonList) {
            if (b.id == OPTIONS_BUTTON_ID) {
                y = b.yPosition;
                break;
            }
        }
        event.buttonList.add(new GuiButton(BUTTON_ID, event.gui.width / 2 + 104, y, 40, 20, "Skin"));
    }

    @SubscribeEvent
    public void onAction(GuiScreenEvent.ActionPerformedEvent.Pre event) {
        if (event.gui instanceof GuiMainMenu && event.button != null && event.button.id == BUTTON_ID) {
            Minecraft.getMinecraft().displayGuiScreen(new SkinScreen(client, skins, event.gui));
            event.setCanceled(true);
        }
    }
}
