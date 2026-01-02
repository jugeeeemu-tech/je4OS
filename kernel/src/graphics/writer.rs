//! Per-task Writer

use super::buffer::{DrawCommand, SharedBuffer};
use super::region::Region;
use alloc::vec::Vec;

/// タスクごとのWriter
///
/// 各タスクが独自のWriterインスタンスを持ち、
/// 描画コマンドをローカルバッファに蓄積し、
/// flush()で共有バッファに一括転送します。
///
/// これにより、1フレームの描画で1回のロック取得のみで済み、
/// ロック競合を大幅に削減します。
pub struct TaskWriter {
    /// 共有バッファへの参照
    buffer: SharedBuffer,
    /// ローカルコマンドバッファ（ロックなしで追加可能）
    local_commands: Vec<DrawCommand>,
    /// 描画領域（領域チェック用にキャッシュ）
    region: Region,
    /// カーソル位置（ローカル座標）
    cursor_x: u32,
    cursor_y: u32,
    /// 現在の文字色
    color: u32,
}

impl TaskWriter {
    /// 新しいWriterを作成
    ///
    /// # Arguments
    /// * `buffer` - 共有バッファへの参照
    /// * `color` - 初期文字色
    pub fn new(buffer: SharedBuffer, color: u32) -> Self {
        // 共有バッファからregionを取得してキャッシュ
        let region = buffer.lock().region();
        Self {
            buffer,
            local_commands: Vec::with_capacity(64),
            region,
            cursor_x: 0,
            cursor_y: 0,
            color,
        }
    }

    /// カーソル位置を設定
    ///
    /// # Arguments
    /// * `x` - X座標（ローカル座標）
    /// * `y` - Y座標（ローカル座標）
    pub fn set_position(&mut self, x: u32, y: u32) {
        self.cursor_x = x;
        self.cursor_y = y;
    }

    /// 文字色を設定
    ///
    /// # Arguments
    /// * `color` - 新しい文字色（0xRRGGBB形式）
    pub fn set_color(&mut self, color: u32) {
        self.color = color;
    }

    /// 領域をクリア
    ///
    /// ローカルバッファにClearコマンドを追加します。
    /// 実際の描画はflush()呼び出し時に行われます。
    ///
    /// # Arguments
    /// * `bg_color` - 背景色
    pub fn clear(&mut self, bg_color: u32) {
        self.local_commands
            .push(DrawCommand::Clear { color: bg_color });
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    /// ローカルバッファのコマンドを共有バッファに一括転送
    ///
    /// この呼び出しでのみ共有バッファのロックを取得します。
    /// 1フレームの描画の最後に呼び出してください。
    pub fn flush(&mut self) {
        if self.local_commands.is_empty() {
            return;
        }

        let mut buf = self.buffer.lock();
        for cmd in self.local_commands.drain(..) {
            buf.push_command(cmd);
        }
    }

    /// 改行処理
    fn newline(&mut self) {
        self.cursor_x = 0;
        self.cursor_y += 10; // 行の高さ（8ピクセル文字 + 2ピクセル間隔）
    }
}

impl core::fmt::Write for TaskWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        // ローカルバッファに追加（ロックなし）
        for ch in s.bytes() {
            if ch == b'\n' {
                // 改行処理（インライン展開）
                self.cursor_x = 0;
                self.cursor_y += 10;
            } else {
                // 領域内に収まるかチェック
                if self.cursor_x + 8 > self.region.width {
                    // 改行処理（インライン展開）
                    self.cursor_x = 0;
                    self.cursor_y += 10;
                }

                // 縦方向のオーバーフロー処理
                if self.cursor_y + 8 > self.region.height {
                    // 領域をクリアして先頭に戻る
                    self.local_commands
                        .push(DrawCommand::Clear { color: 0x00000000 });
                    self.cursor_y = 0;
                }

                self.local_commands.push(DrawCommand::DrawChar {
                    x: self.cursor_x,
                    y: self.cursor_y,
                    ch,
                    color: self.color,
                });

                self.cursor_x += 8;
            }
        }
        Ok(())
    }
}
