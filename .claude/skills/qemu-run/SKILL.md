---
name: qemu-run
description: Run vitrOS kernel in QEMU with GDB boot detection. Use when user wants to run/test the kernel. Triggers on "run", "execute", "test", "cargo run", "QEMU", "動かす", "実行", "テスト", "起動".
allowed-tools: Read, Bash
---

# QEMU Run - カーネル起動と動作確認

このスキルは、QEMUでVitrOSカーネルを起動し、GDBブレークポイントで起動成功を確認します。

## 起動手順

### 1. 既存プロセスの停止とログのクリア

```bash
cd /home/jugeeeemu/VitrOS
pkill -f "qemu-system-x86_64.*fat:rw:mnt" 2>/dev/null || true
rm -f serial.log qemu_debug.log qemu_stdout.log
```

### 2. cargo run でビルド＆起動

```bash
cd /home/jugeeeemu/VitrOS
nohup env ENABLE_GDB=1 GDB_WAIT=1 QEMU_DEBUG_LOG=1 cargo run \
  > qemu_stdout.log 2>&1 &
echo $! > .qemu.pid
```

**環境変数**:
- `ENABLE_GDB=1`: GDBサーバー有効（ポート1234）
- `GDB_WAIT=1`: GDB接続まで実行停止
- `QEMU_DEBUG_LOG=1`: デバッグログ有効

### 3. GDBで起動完了を検出

```bash
gdb -batch \
  -ex "target remote :1234" \
  -ex "hbreak boot_complete" \
  -ex "continue" \
  -ex "echo \nBOOT_COMPLETE\n" \
  /home/jugeeeemu/VitrOS/target/x86_64-unknown-none/debug/vitros-kernel
```

- `hbreak`: ハードウェアブレークポイント（カーネル起動前でも有効）
- `BOOT_COMPLETE` が出力されれば起動成功
- GDB終了後もQEMUは継続実行

## 動作確認（GDBブレークポイント継続使用）

起動成功後、実装した機能の動作を確認するため、追加のブレークポイントを設定：

### タイマー動作確認

```bash
gdb -batch \
  -ex "target remote :1234" \
  -ex "hbreak timer_handler" \
  -ex "continue" \
  -ex "echo \nTIMER_HANDLER_CALLED\n" \
  /home/jugeeeemu/VitrOS/target/x86_64-unknown-none/debug/vitros-kernel
```

### タスク切り替え確認

```bash
gdb -batch \
  -ex "target remote :1234" \
  -ex "hbreak switch_context" \
  -ex "continue" \
  -ex "echo \nTASK_SWITCH_OCCURRED\n" \
  /home/jugeeeemu/VitrOS/target/x86_64-unknown-none/debug/vitros-kernel
```

### 確認対象の関数例

- タイマー: `timer_handler`, `process_pending_timers`
- タスク切り替え: `switch_context`, `schedule`
- 割り込み: `divide_error_handler`, `page_fault_handler`

## serial.log 確認（補助的）

GDB確認後、詳細なログを確認したい場合：

```bash
cat /home/jugeeeemu/VitrOS/serial.log
```

## 停止

```bash
pkill -f "qemu-system-x86_64.*fat:rw:mnt"
rm -f /home/jugeeeemu/VitrOS/.qemu.pid
```

## 責務

- **やること**: カーネルの起動、起動成功の確認、実装した機能の動作確認、停止
- **やらないこと**: 詳細なデバッグ（`qemu-gdb-debug` スキルに任せる）

## 注意事項

- QEMUはGDB接続待機状態で起動するため、GDBで `continue` を実行するまで停止している
- 起動成功後もGDB接続は維持されるため、追加のブレークポイントで動作確認可能
- 詳細なデバッグが必要な場合は `qemu-gdb-debug` スキルを使用
