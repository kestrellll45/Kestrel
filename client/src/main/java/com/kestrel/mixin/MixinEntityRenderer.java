package com.kestrel.mixin;

import com.kestrel.rendering.Fullbright;
import net.minecraft.client.renderer.EntityRenderer;
import net.minecraft.client.settings.GameSettings;
import org.objectweb.asm.Opcodes;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/**
 * Fullbright hook. {@code updateLightmap} reads {@code gameSettings.gammaSetting}
 * exactly once (verified against the 1.8.9 bytecode); only that read is
 * redirected, so the stored setting and options.txt never change.
 */
@Mixin(EntityRenderer.class)
public abstract class MixinEntityRenderer {
    @Redirect(
            method = "updateLightmap",
            at = @At(value = "FIELD",
                    target = "Lnet/minecraft/client/settings/GameSettings;gammaSetting:F",
                    opcode = Opcodes.GETFIELD))
    private float kestrel$lightmapGamma(GameSettings settings) {
        return Fullbright.lightmapGamma(settings.gammaSetting);
    }
}
