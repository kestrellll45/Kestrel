package com.kestrel.util;

import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.util.ResourceLocation;

import javax.imageio.ImageIO;
import javax.imageio.ImageReader;
import javax.imageio.stream.ImageInputStream;
import java.awt.image.BufferedImage;
import java.io.File;
import java.io.IOException;
import java.util.Iterator;

/**
 * Decodes validated user PNGs (off the game thread) and uploads them as
 * dynamic textures (on the game thread).
 */
public final class TextureLoader {
    private TextureLoader() {
    }

    /**
     * Decode a PNG whose header has already been validated. Uses the PNG
     * reader explicitly so no other ImageIO plugin parses untrusted data.
     * Call off the game thread.
     */
    public static BufferedImage decodePng(File file) throws IOException {
        ImageInputStream in = ImageIO.createImageInputStream(file);
        if (in == null) {
            throw new IOException("cannot open " + file.getName());
        }
        try {
            Iterator<ImageReader> readers = ImageIO.getImageReadersByFormatName("png");
            if (!readers.hasNext()) {
                throw new IOException("no PNG decoder available");
            }
            ImageReader reader = readers.next();
            try {
                reader.setInput(in, true, true);
                return reader.read(0);
            } finally {
                reader.dispose();
            }
        } finally {
            in.close();
        }
    }

    /** Upload an image as a texture. Must run on the game thread. */
    public static ResourceLocation upload(String name, BufferedImage image) {
        DynamicTexture tex = new DynamicTexture(image);
        return Minecraft.getMinecraft().getTextureManager().getDynamicTextureLocation(name, tex);
    }

    /** Free a texture created by {@link #upload}. Game thread only. */
    public static void delete(ResourceLocation loc) {
        if (loc != null) {
            Minecraft.getMinecraft().getTextureManager().deleteTexture(loc);
        }
    }

    /** Whether a resource (e.g. a bundled texture) exists. */
    public static boolean resourceExists(ResourceLocation loc) {
        try {
            Minecraft.getMinecraft().getResourceManager().getResource(loc).getInputStream().close();
            return true;
        } catch (IOException e) {
            return false;
        }
    }
}
