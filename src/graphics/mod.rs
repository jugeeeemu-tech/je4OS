mod font;

pub use font::FONT_8X8;

// フレームバッファに文字を描画
pub fn draw_char(fb_base: u64, width: u32, x: usize, y: usize, ch: u8, color: u32) {
    let fb_ptr = fb_base as *mut u32;

    if ch < 32 || ch > 126 {
        return; // サポート外の文字
    }

    let font_index = (ch - 32) as usize;
    let glyph = FONT_8X8[font_index];

    unsafe {
        for row in 0..8 {
            for col in 0..8 {
                if (glyph[row] >> col) & 1 == 1 {
                    let pixel_offset = (y + row) * width as usize + (x + col);
                    *fb_ptr.add(pixel_offset) = color;
                }
            }
        }
    }
}

// 文字列を描画
pub fn draw_string(fb_base: u64, width: u32, x: usize, y: usize, s: &str, color: u32) {
    let mut cur_x = x;
    for ch in s.bytes() {
        draw_char(fb_base, width, cur_x, y, ch, color);
        cur_x += 8; // 次の文字へ（8ピクセル幅）
    }
}
