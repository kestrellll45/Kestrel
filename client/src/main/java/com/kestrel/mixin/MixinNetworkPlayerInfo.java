package com.kestrel.mixin;

import com.kestrel.skin.SkinHooks;
import net.minecraft.client.network.NetworkPlayerInfo;
import net.minecraft.util.ResourceLocation;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Local skin changer: the local player's tab-list entry (head icon). */
@Mixin(NetworkPlayerInfo.class)
public abstract class MixinNetworkPlayerInfo {
    @Inject(method = "getLocationSkin", at = @At("HEAD"), cancellable = true, require = 1)
    private void kestrel$localSkin(CallbackInfoReturnable<ResourceLocation> cir) {
        ResourceLocation r = SkinHooks.skin(((NetworkPlayerInfo) (Object) this).getGameProfile());
        if (r != null) cir.setReturnValue(r);
    }

    @Inject(method = "getSkinType", at = @At("HEAD"), cancellable = true, require = 1)
    private void kestrel$localSkinModel(CallbackInfoReturnable<String> cir) {
        String m = SkinHooks.model(((NetworkPlayerInfo) (Object) this).getGameProfile());
        if (m != null) cir.setReturnValue(m);
    }

    @Inject(method = "hasLocationSkin", at = @At("HEAD"), cancellable = true, require = 1)
    private void kestrel$hasLocalSkin(CallbackInfoReturnable<Boolean> cir) {
        if (SkinHooks.skin(((NetworkPlayerInfo) (Object) this).getGameProfile()) != null) cir.setReturnValue(true);
    }
}
