package com.kestrel.mixin;

import com.kestrel.camera.CameraHooks;
import net.minecraft.client.renderer.entity.RenderManager;
import net.minecraft.entity.Entity;
import org.objectweb.asm.Opcodes;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/**
 * Free-look: name tags and other entity billboards face
 * {@code RenderManager.playerViewX/Y}, which {@code cacheActiveRenderInfo}
 * derives from the view entity's rotation. Return the camera's angles there.
 */
@Mixin(RenderManager.class)
public abstract class MixinRenderManager {
    @Redirect(method = "cacheActiveRenderInfo", require = 1,
            at = @At(value = "FIELD", target = "Lnet/minecraft/entity/Entity;rotationYaw:F", opcode = Opcodes.GETFIELD))
    private float kestrel$viewYaw(Entity e) {
        return CameraHooks.yaw(e, e.rotationYaw);
    }

    @Redirect(method = "cacheActiveRenderInfo", require = 1,
            at = @At(value = "FIELD", target = "Lnet/minecraft/entity/Entity;prevRotationYaw:F", opcode = Opcodes.GETFIELD))
    private float kestrel$viewPrevYaw(Entity e) {
        return CameraHooks.yaw(e, e.prevRotationYaw);
    }

    @Redirect(method = "cacheActiveRenderInfo", require = 1,
            at = @At(value = "FIELD", target = "Lnet/minecraft/entity/Entity;rotationPitch:F", opcode = Opcodes.GETFIELD))
    private float kestrel$viewPitch(Entity e) {
        return CameraHooks.pitch(e, e.rotationPitch);
    }

    @Redirect(method = "cacheActiveRenderInfo", require = 1,
            at = @At(value = "FIELD", target = "Lnet/minecraft/entity/Entity;prevRotationPitch:F", opcode = Opcodes.GETFIELD))
    private float kestrel$viewPrevPitch(Entity e) {
        return CameraHooks.pitch(e, e.prevRotationPitch);
    }
}
