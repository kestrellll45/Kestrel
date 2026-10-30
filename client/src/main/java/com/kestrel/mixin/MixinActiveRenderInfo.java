package com.kestrel.mixin;

import com.kestrel.camera.CameraHooks;
import net.minecraft.client.renderer.ActiveRenderInfo;
import net.minecraft.entity.player.EntityPlayer;
import net.minecraft.util.MathHelper;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Free-look: particles and other billboards face the camera using the
 * rotation computed in {@code ActiveRenderInfo.updateRenderInfo}. While
 * free-look is active, recompute it from the camera's angles with exactly
 * vanilla's formula (verified against the 1.8.9 bytecode).
 */
@Mixin(ActiveRenderInfo.class)
public abstract class MixinActiveRenderInfo {
    @Shadow
    private static float rotationX;
    @Shadow
    private static float rotationXZ;
    @Shadow
    private static float rotationZ;
    @Shadow
    private static float rotationYZ;
    @Shadow
    private static float rotationXY;

    @Inject(method = "updateRenderInfo", at = @At("RETURN"), require = 1)
    private static void kestrel$freeLookBillboards(EntityPlayer player, boolean frontView, CallbackInfo ci) {
        if (!CameraHooks.overrides(player)) return;
        float pitch = CameraHooks.pitch(player, 0f);
        float yaw = CameraHooks.yaw(player, 0f);
        float sign = frontView ? -1f : 1f;
        float yawRad = yaw * (float) Math.PI / 180.0f;
        float pitchRad = pitch * (float) Math.PI / 180.0f;
        rotationX = MathHelper.cos(yawRad) * sign;
        rotationZ = MathHelper.sin(yawRad) * sign;
        rotationYZ = -rotationZ * MathHelper.sin(pitchRad) * sign;
        rotationXY = rotationX * MathHelper.sin(pitchRad) * sign;
        rotationXZ = MathHelper.cos(pitchRad);
    }
}
