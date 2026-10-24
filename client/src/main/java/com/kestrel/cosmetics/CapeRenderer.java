package com.kestrel.cosmetics;

import com.kestrel.KestrelClient;
import net.minecraft.client.Minecraft;
import net.minecraft.client.entity.AbstractClientPlayer;
import net.minecraft.client.renderer.GlStateManager;
import net.minecraft.entity.player.EnumPlayerModelParts;
import net.minecraft.util.MathHelper;
import net.minecraft.util.ResourceLocation;
import net.minecraftforge.client.event.RenderPlayerEvent;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;

/**
 * Renders the selected cape on the local player.
 *
 * <p>Uses only Forge's RenderPlayerEvent (no mixins): after the player is
 * drawn we rebuild the same model transform RendererLivingEntity uses and
 * then run vanilla LayerCape's motion maths, so the cape swings exactly like
 * a vanilla cape. If the player already has an official cape, vanilla draws
 * that one and we draw nothing (no z-fighting).
 */
public final class CapeRenderer {
    private final KestrelClient client;

    public CapeRenderer(KestrelClient client) {
        this.client = client;
    }

    @SubscribeEvent
    public void onRenderPlayer(RenderPlayerEvent.Post event) {
        if (!(event.entityPlayer instanceof AbstractClientPlayer)) return;
        AbstractClientPlayer player = (AbstractClientPlayer) event.entityPlayer;
        Minecraft mc = Minecraft.getMinecraft();
        if (player != mc.thePlayer) return;
        ResourceLocation tex = client.capes().currentTexture();
        if (tex == null) return; // disabled or nothing selected: no GL work at all
        if (player.isInvisible() || !player.isWearing(EnumPlayerModelParts.CAPE)) return;
        if (player.getLocationCape() != null) return;

        float pt = event.partialRenderTick;
        GlStateManager.pushMatrix();
        GlStateManager.color(1f, 1f, 1f, 1f);
        GlStateManager.enableRescaleNormal();
        GlStateManager.enableLighting();

        // RendererLivingEntity.doRender: renderLivingAt + rotateCorpse + scale.
        GlStateManager.translate((float) event.x, (float) event.y, (float) event.z);
        float bodyYaw = interpolateRotation(player.prevRenderYawOffset, player.renderYawOffset, pt);
        GlStateManager.rotate(180.0F - bodyYaw, 0.0F, 1.0F, 0.0F);
        GlStateManager.scale(-1.0F, -1.0F, 1.0F);
        GlStateManager.scale(0.9375F, 0.9375F, 0.9375F); // RenderPlayer.preRenderCallback
        GlStateManager.translate(0.0F, -1.5078125F, 0.0F);

        // LayerCape.doRenderLayer (1.8.9).
        mc.getTextureManager().bindTexture(tex);
        GlStateManager.translate(0.0F, 0.0F, 0.125F);
        double dx = player.prevChasingPosX + (player.chasingPosX - player.prevChasingPosX) * pt
                - (player.prevPosX + (player.posX - player.prevPosX) * pt);
        double dy = player.prevChasingPosY + (player.chasingPosY - player.prevChasingPosY) * pt
                - (player.prevPosY + (player.posY - player.prevPosY) * pt);
        double dz = player.prevChasingPosZ + (player.chasingPosZ - player.prevChasingPosZ) * pt
                - (player.prevPosZ + (player.posZ - player.prevPosZ) * pt);
        float yaw = player.prevRenderYawOffset + (player.renderYawOffset - player.prevRenderYawOffset) * pt;
        double sin = MathHelper.sin(yaw * (float) Math.PI / 180.0F);
        double cos = -MathHelper.cos(yaw * (float) Math.PI / 180.0F);
        float lift = MathHelper.clamp_float((float) dy * 10.0F, -6.0F, 32.0F);
        float swing = (float) (dx * sin + dz * cos) * 100.0F;
        float side = (float) (dx * cos - dz * sin) * 100.0F;
        if (swing < 0.0F) swing = 0.0F;
        float bob = player.prevCameraYaw + (player.cameraYaw - player.prevCameraYaw) * pt;
        lift += MathHelper.sin((player.prevDistanceWalkedModified
                + (player.distanceWalkedModified - player.prevDistanceWalkedModified) * pt) * 6.0F) * 32.0F * bob;
        if (player.isSneaking()) lift += 25.0F;
        GlStateManager.rotate(6.0F + swing / 2.0F + lift, 1.0F, 0.0F, 0.0F);
        GlStateManager.rotate(side / 2.0F, 0.0F, 0.0F, 1.0F);
        GlStateManager.rotate(-side / 2.0F, 0.0F, 1.0F, 0.0F);
        GlStateManager.rotate(180.0F, 0.0F, 1.0F, 0.0F);
        event.renderer.getMainModel().renderCape(0.0625F);

        GlStateManager.disableRescaleNormal();
        GlStateManager.popMatrix();
    }

    private static float interpolateRotation(float prev, float cur, float pt) {
        float d = MathHelper.wrapAngleTo180_float(cur - prev);
        return prev + pt * d;
    }
}
