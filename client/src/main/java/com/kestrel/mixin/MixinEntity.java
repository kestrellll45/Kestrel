package com.kestrel.mixin;

import com.kestrel.camera.CameraHooks;
import net.minecraft.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Mouse look. In 1.8.9 {@code Entity.setAngles(yaw, pitch)} is how
 * {@code EntityRenderer.updateCameraAndRender} applies mouse movement to the
 * player. While free-look is held the movement turns the free-look camera
 * instead (the player's rotation is untouched); with zoom sensitivity scaling
 * on, the deltas are scaled. Otherwise vanilla runs unchanged.
 */
@Mixin(Entity.class)
public abstract class MixinEntity {
    @Inject(method = "setAngles", at = @At("HEAD"), cancellable = true, require = 1)
    private void kestrel$mouseLook(float yaw, float pitch, CallbackInfo ci) {
        if (CameraHooks.onSetAngles(this, yaw, pitch)) {
            ci.cancel();
        }
    }
}
