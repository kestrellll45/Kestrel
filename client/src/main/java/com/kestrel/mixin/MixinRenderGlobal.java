package com.kestrel.mixin;

import com.kestrel.camera.CameraHooks;
import net.minecraft.client.renderer.RenderGlobal;
import net.minecraft.entity.Entity;
import org.objectweb.asm.Opcodes;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/**
 * Free-look: chunk visibility. {@code setupTerrain} only recomputes which
 * chunks are visible when the view entity moves or turns, and
 * {@code getViewVector} gives the direction the visibility search starts
 * from. Both read the entity's rotation; return the camera's angles so
 * looking around (including behind you) never leaves chunks missing.
 */
@Mixin(RenderGlobal.class)
public abstract class MixinRenderGlobal {
    @Redirect(method = {"setupTerrain", "getViewVector"}, require = 2,
            at = @At(value = "FIELD", target = "Lnet/minecraft/entity/Entity;rotationYaw:F", opcode = Opcodes.GETFIELD))
    private float kestrel$viewYaw(Entity e) {
        return CameraHooks.yaw(e, e.rotationYaw);
    }

    @Redirect(method = {"setupTerrain", "getViewVector"}, require = 2,
            at = @At(value = "FIELD", target = "Lnet/minecraft/entity/Entity;rotationPitch:F", opcode = Opcodes.GETFIELD))
    private float kestrel$viewPitch(Entity e) {
        return CameraHooks.pitch(e, e.rotationPitch);
    }

    @Redirect(method = "getViewVector", require = 1,
            at = @At(value = "FIELD", target = "Lnet/minecraft/entity/Entity;prevRotationYaw:F", opcode = Opcodes.GETFIELD))
    private float kestrel$viewPrevYaw(Entity e) {
        return CameraHooks.yaw(e, e.prevRotationYaw);
    }

    @Redirect(method = "getViewVector", require = 1,
            at = @At(value = "FIELD", target = "Lnet/minecraft/entity/Entity;prevRotationPitch:F", opcode = Opcodes.GETFIELD))
    private float kestrel$viewPrevPitch(Entity e) {
        return CameraHooks.pitch(e, e.prevRotationPitch);
    }
}
