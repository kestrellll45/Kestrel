package com.kestrel.gui.widgets;

import com.kestrel.gui.Theme;
import com.kestrel.util.RenderUtil;
import net.minecraft.client.Minecraft;
import net.minecraft.client.audio.PositionedSoundRecord;
import net.minecraft.util.ResourceLocation;
import org.lwjgl.input.Keyboard;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.function.Consumer;
import java.util.function.Supplier;

/**
 * A filterable grid of selectable tiles with previews: category chips, a
 * search box and one tile per item (capes, skies, kill effects). The selected
 * item is outlined in the accent colour and labelled "EQUIPPED".
 */
public final class TileGrid extends Widget {
    /** One selectable item. */
    public static final class Item {
        public final String id;
        public final String name;
        public final String category;
        /** Small corner badge (e.g. "ANIM"), or null. */
        public final String badge;

        public Item(String id, String name, String category, String badge) {
            this.id = id;
            this.name = name;
            this.category = category;
            this.badge = badge;
        }
    }

    /** Draws an item's preview inside the tile's picture area. */
    public interface Painter {
        void paint(Minecraft mc, Item item, int x, int y, int w, int h);
    }

    /** Filter state that survives rebuilding the screen (one per grid kind). */
    public static final class Filter {
        public String category = ""; // "" = all
        public String search = "";
    }

    private static final int CHIP_H = 12;
    private static final int SEARCH_H = 14;
    private static final int GAP = 4;
    private static final int LABEL_H = 11;

    private final List<Item> items;
    private final List<String[]> categories; // {id, label}
    private final Supplier<String> selected;
    private final Consumer<String> select;
    private final Painter painter;
    private final Filter filter;
    private final int picW;
    private final int picH;

    private final List<Item> visible = new ArrayList<Item>();
    private final List<int[]> chipBoxes = new ArrayList<int[]>(); // x0, y0, x1, y1 relative to widget
    private int searchY;
    private int gridY;
    private int cols = 1;
    private boolean searchFocused;

    public TileGrid(List<Item> items, List<String[]> categories, Supplier<String> selected, Consumer<String> select,
                    Painter painter, Filter filter, int picW, int picH) {
        this.items = items;
        this.categories = categories;
        this.selected = selected;
        this.select = select;
        this.painter = painter;
        this.filter = filter;
        this.picW = picW;
        this.picH = picH;
    }

    private int tileW() {
        return picW + 6;
    }

    private int tileH() {
        return picH + LABEL_H + 6;
    }

    /** Recompute filtering and height for the given width (call from the screen's layout). */
    public void relayout(Minecraft mc, int width) {
        w = width;
        // Category chips wrap onto as many rows as needed.
        chipBoxes.clear();
        int cx = 0;
        int cy = 0;
        for (int i = 0; i <= categories.size(); i++) {
            String label = i == 0 ? "All" : categories.get(i - 1)[1];
            int cw = mc.fontRendererObj.getStringWidth(label) + 10;
            if (cx > 0 && cx + cw > width) {
                cx = 0;
                cy += CHIP_H + 3;
            }
            chipBoxes.add(new int[] {cx, cy, cx + cw, cy + CHIP_H});
            cx += cw + 3;
        }
        searchY = cy + CHIP_H + 4;
        gridY = searchY + SEARCH_H + GAP;
        visible.clear();
        String q = filter.search.trim().toLowerCase(Locale.ROOT);
        for (Item it : items) {
            if (!filter.category.isEmpty() && !filter.category.equals(it.category)) continue;
            if (!q.isEmpty() && !it.name.toLowerCase(Locale.ROOT).contains(q)) continue;
            visible.add(it);
        }
        cols = Math.max(1, (width + GAP) / (tileW() + GAP));
        int rows = (visible.size() + cols - 1) / cols;
        h = gridY + Math.max(1, rows) * (tileH() + GAP);
    }

    public boolean isSearchFocused() {
        return searchFocused;
    }

    @Override
    public void draw(Minecraft mc, int mx, int my) {
        // Chips.
        for (int i = 0; i < chipBoxes.size(); i++) {
            int[] b = chipBoxes.get(i);
            String id = i == 0 ? "" : categories.get(i - 1)[0];
            String label = i == 0 ? "All" : categories.get(i - 1)[1];
            boolean on = filter.category.equals(id);
            boolean hov = mx >= x + b[0] && mx < x + b[2] && my >= y + b[1] && my < y + b[3];
            RenderUtil.softRect(x + b[0], y + b[1], x + b[2], y + b[3], on ? Theme.ACCENT : (hov ? Theme.CARD_HOVER : Theme.CARD));
            mc.fontRendererObj.drawString(label, x + b[0] + 5, y + b[1] + 2, on ? 0xFF111111 : Theme.TEXT, false);
        }
        // Search box.
        int sy = y + searchY;
        RenderUtil.softRect(x, sy, x + w, sy + SEARCH_H, 0xFF101114);
        RenderUtil.outline(x, sy, x + w, sy + SEARCH_H, searchFocused ? Theme.ACCENT : Theme.TRACK);
        String text = filter.search;
        boolean caret = searchFocused && (System.currentTimeMillis() / 500) % 2 == 0;
        if (text.isEmpty() && !searchFocused) {
            mc.fontRendererObj.drawString("Search " + items.size() + " items...", x + 5, sy + 3, Theme.MUTED, false);
        } else {
            mc.fontRendererObj.drawString(fit(mc, text, w - 16) + (caret ? "_" : ""), x + 5, sy + 3, Theme.TEXT, false);
        }
        String count = visible.size() + "/" + items.size();
        mc.fontRendererObj.drawString(count, x + w - mc.fontRendererObj.getStringWidth(count) - 5, sy + 3, Theme.MUTED, false);
        // Tiles.
        String sel = selected.get();
        if (visible.isEmpty()) {
            mc.fontRendererObj.drawString("Nothing matches.", x + 4, y + gridY + 4, Theme.MUTED, false);
        }
        for (int i = 0; i < visible.size(); i++) {
            Item it = visible.get(i);
            int tx = x + (i % cols) * (tileW() + GAP);
            int ty = y + gridY + (i / cols) * (tileH() + GAP);
            boolean on = it.id.equals(sel);
            boolean hov = mx >= tx && mx < tx + tileW() && my >= ty && my < ty + tileH();
            RenderUtil.softRect(tx, ty, tx + tileW(), ty + tileH(), on ? 0xFF3A2A1E : (hov ? Theme.CARD_HOVER : Theme.CARD));
            if (on) RenderUtil.outline(tx, ty, tx + tileW(), ty + tileH(), Theme.ACCENT);
            else if (hov) RenderUtil.outline(tx, ty, tx + tileW(), ty + tileH(), Theme.ACCENT_DIM);
            painter.paint(mc, it, tx + 3, ty + 3, picW, picH);
            if (it.badge != null) {
                int bw = mc.fontRendererObj.getStringWidth(it.badge) + 4;
                RenderUtil.rect(tx + tileW() - bw - 2, ty + 2, tx + tileW() - 2, ty + 11, 0xC0000000);
                mc.fontRendererObj.drawString(it.badge, tx + tileW() - bw, ty + 3, Theme.ACCENT, false);
            }
            String label = on ? "\u00bb " + it.name : it.name;
            mc.fontRendererObj.drawString(fit(mc, label, tileW() - 4), tx + 3, ty + picH + 5, on ? Theme.ACCENT : Theme.TEXT, false);
        }
    }

    @Override
    public boolean mouseClicked(int mx, int my, int button) {
        if (button != 0 || !isHovered(mx, my)) {
            searchFocused = false;
            return false;
        }
        for (int i = 0; i < chipBoxes.size(); i++) {
            int[] b = chipBoxes.get(i);
            if (mx >= x + b[0] && mx < x + b[2] && my >= y + b[1] && my < y + b[3]) {
                filter.category = i == 0 ? "" : categories.get(i - 1)[0];
                click();
                relayout(Minecraft.getMinecraft(), w);
                return true;
            }
        }
        int sy = y + searchY;
        if (my >= sy && my < sy + SEARCH_H) {
            searchFocused = true;
            return true;
        }
        searchFocused = false;
        for (int i = 0; i < visible.size(); i++) {
            int tx = x + (i % cols) * (tileW() + GAP);
            int ty = y + gridY + (i / cols) * (tileH() + GAP);
            if (mx >= tx && mx < tx + tileW() && my >= ty && my < ty + tileH()) {
                click();
                select.accept(visible.get(i).id);
                return true;
            }
        }
        return false;
    }

    /**
     * Keyboard input while the search box is focused.
     *
     * @return true if the key was consumed
     */
    public boolean keyTyped(char c, int key) {
        if (!searchFocused) return false;
        if (key == Keyboard.KEY_ESCAPE || key == Keyboard.KEY_RETURN) {
            searchFocused = false;
            return true;
        }
        if (key == Keyboard.KEY_BACK) {
            if (!filter.search.isEmpty()) filter.search = filter.search.substring(0, filter.search.length() - 1);
        } else if (c >= ' ' && c != 127 && c != '\u00a7' && filter.search.length() < 32) {
            filter.search += c;
        } else {
            return true;
        }
        relayout(Minecraft.getMinecraft(), w);
        return true;
    }

    private static void click() {
        Minecraft.getMinecraft().getSoundHandler().playSound(PositionedSoundRecord.create(new ResourceLocation("gui.button.press"), 1.0F));
    }
}
