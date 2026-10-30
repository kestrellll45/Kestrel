package com.kestrel.mixin;

import com.kestrel.camera.CameraHooks;
import com.kestrel.rendering.Fullbright;
import com.kestrel.rendering.StaticFov;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.EntityRenderer;
import net.minecraft.client.settings.GameSettings;
import net.minecraft.entity.Entity;
import org.objectweb.asm.Opcodes;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Rendering hooks on {@link EntityRenderer} (all verified against the 1.8.9
 * Forge bytecode).
 */
@Mixin(EntityRenderer.class)
public abstract class MixinEntityRenderer {
    @Shadow
    private Minecraft mc;

    @Shadow
    private boolean debugView;

    /**
     * Fullbright. {@code updateLightmap} reads {@code gameSettings.gammaSetting}
     * exactly once; only that read is redirected, so the stored setting and
     * options.txt never change.
     */
    @Redirect(
            method = "updateLightmap",
            at = @At(value = "FIELD",
                    target = "Lnet/minecraft/client/settings/GameSettings;gammaSetting:F",
                    opcode = Opcodes.GETFIELD))
    private float kestrel$lightmapGamma(GameSettings settings) {
        return Fullbright.lightmapGamma(settings.gammaSetting);
    }

    /**
     * No dynamic FOV: sprinting, speed/slowness, flying, bows, death, water
     * and mod FOV events no longer change the field of view (see
     * {@link StaticFov}). Read every frame from the live setting, so the FOV
     * slider still applies immediately. Hold-to-zoom multiplies the world FOV
     * for the current frame only ({@link CameraHooks#fovMultiplier()}).
     */
    @Inject(method = "getFOVModifier", at = @At("HEAD"), cancellable = true)
    private void kestrel$staticFov(float partialTicks, boolean useFOVSetting,
                                   CallbackInfoReturnable<Float> cir) {
        cir.setReturnValue(StaticFov.fov(this.debugView, useFOVSetting, this.mc.gameSettings.fovSetting,
                CameraHooks.fovMultiplier()));
    }

    // Free-look: orientCamera reads the view entity's rotation (first person,
    // third person and the Forge CameraSetup event all derive from these
    // reads). While free-look is active they return the camera's own angles
    // for the local player; every other entity and every other time they
    // return the real field, so nothing changes when free-look is off.

    @Redirect(method = "orientCamera", require = 1,
            at = @At(value = "FIELD", target = "Lnet/minecraft/entity/Entity;rotationYaw:F", opcode = Opcodes.GETFIELD))
    private float kestrel$cameraYaw(Entity e) {
        return CameraHooks.yaw(e, e.rotationYaw);
    }

    @Redirect(method = "orientCamera", require = 1,
            at = @At(value = "FIELD", target = "Lnet/minecraft/entity/Entity;prevRotationYaw:F", opcode = Opcodes.GETFIELD))
    private float kestrel$cameraPrevYaw(Entity e) {
        return CameraHooks.yaw(e, e.prevRotationYaw);
    }

    @Redirect(method = "orientCamera", require = 1,
            at = @At(value = "FIELD", target = "Lnet/minecraft/entity/Entity;rotationPitch:F", opcode = Opcodes.GETFIELD))
    private float kestrel$cameraPitch(Entity e) {
        return CameraHooks.pitch(e, e.rotationPitch);
    }

    @Redirect(method = "orientCamera", require = 1,
            at = @At(value = "FIELD", target = "Lnet/minecraft/entity/Entity;prevRotationPitch:F", opcode = Opcodes.GETFIELD))
    private float kestrel$cameraPrevPitch(Entity e) {
        return CameraHooks.pitch(e, e.prevRotationPitch);
    }
}
