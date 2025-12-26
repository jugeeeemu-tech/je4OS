// =============================================================================
// メモリアロケータ可視化機能
// cargo build --release --features visualize でビルドした場合のみ有効
// =============================================================================

use crate::allocator;
use crate::graphics::{draw_rect, draw_rect_outline, draw_string, FramebufferWriter};
use alloc::format;

// アドレスをアラインメントに合わせて切り下げ
fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}

// 画面左側にコードスニペットを表示
pub fn draw_code_snippet(writer: &mut FramebufferWriter, code_lines: &[&str]) {
    let fb_base = writer.fb_base;
    let screen_width = writer.width;

    // 左側の領域をクリア
    draw_rect(fb_base, screen_width, 0, 280, 400, 320, 0x000000);

    let start_x = 10;
    let mut y = 290;

    // タイトル
    draw_string(fb_base, screen_width, start_x, y, "Code:", 0xFFFF00);
    y += 15;

    // コード行を描画
    for line in code_lines {
        draw_string(fb_base, screen_width, start_x, y, line, 0x00FFFF);
        y += 10;
    }
}

// 複数のサイズクラスをコンパクトに並べて表示
pub fn draw_memory_grids_multi(writer: &mut FramebufferWriter, title: &str) {
    let allocator = allocator::get_allocator();
    let size_classes = allocator::get_size_classes();

    let fb_base = writer.fb_base;
    let screen_width = writer.width;

    // 右側の領域をクリア（x=400以降）
    draw_rect(fb_base, screen_width, 400, 280, 624, 320, 0x000000);

    // タイトルを描画
    draw_string(fb_base, screen_width, 410, 290, title, 0xFFFF00);

    let heap_size = 256 * 1024; // 256KB

    // 各サイズクラスを3列で並べて表示（最大6個まで）
    let grid_cols_per_class = 20; // 各グリッドは20x20セル
    let cell_size = 3; // 各セル3x3ピクセル
    let grid_pixel_size = grid_cols_per_class * (cell_size + 1); // 約80ピクセル

    let start_x = 410;
    let start_y = 310;
    let classes_to_show = 6.min(size_classes.len()); // 画面に収まる範囲で6個まで

    for class_idx in 0..classes_to_show {
        let size = size_classes[class_idx];
        let slab_size = (heap_size / 2) / size_classes.len();
        let aligned_size = align_down(slab_size, size);
        let total_blocks = aligned_size / size;

        let free_count = allocator.count_free_blocks(class_idx);
        let used_count = total_blocks - free_count;

        // グリッドの位置を計算（3列レイアウト）
        let col = class_idx % 3;
        let row = class_idx / 3;
        let grid_x = start_x + col * (grid_pixel_size + 20);
        let grid_y = start_y + row * (grid_pixel_size + 35);

        // サイズクラスラベル
        let label = format!("{}B", size);
        draw_string(fb_base, screen_width, grid_x, grid_y - 12, &label, 0xFFFFFF);

        // グリッドを描画（最大400ブロックまで = 20x20）
        let max_display = (grid_cols_per_class * grid_cols_per_class).min(total_blocks);

        for i in 0..max_display {
            let grid_row = i / grid_cols_per_class;
            let grid_col = i % grid_cols_per_class;

            let x = grid_x + grid_col * (cell_size + 1);
            let y = grid_y + grid_row * (cell_size + 1);

            let color = if i < used_count {
                0xFF0000 // 赤: 使用中
            } else {
                0x00FF00 // 緑: 空き
            };

            draw_rect(fb_base, screen_width, x, y, cell_size, cell_size, color);
        }

        // 使用率を表示
        let usage_pct = if total_blocks > 0 {
            (used_count * 100) / total_blocks
        } else {
            0
        };
        let usage = format!("{}%", usage_pct);
        draw_string(fb_base, screen_width, grid_x + 25, grid_y + grid_pixel_size + 3, &usage, 0xAAAAAA);
    }

    // 凡例
    let legend_y = start_y + 2 * (grid_pixel_size + 35) + 5;
    draw_rect(fb_base, screen_width, start_x, legend_y, 8, 8, 0xFF0000);
    draw_string(fb_base, screen_width, start_x + 12, legend_y, "Used", 0xFFFFFF);
    draw_rect(fb_base, screen_width, start_x + 60, legend_y, 8, 8, 0x00FF00);
    draw_string(fb_base, screen_width, start_x + 72, legend_y, "Free", 0xFFFFFF);
}
