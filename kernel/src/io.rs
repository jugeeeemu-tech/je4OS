// x86_64 I/Oポート操作と割り込み制御
use core::arch::asm;

/// 割り込みを無効化してクロージャを実行し、元の状態に復元する
///
/// クロージャ実行後、元の割り込み状態を復元します。
/// 割り込みハンドラからアクセスされる可能性のあるロックを取得する際に使用します。
///
/// # Arguments
/// * `f` - 割り込み無効状態で実行するクロージャ
///
/// # Returns
/// クロージャの戻り値
#[inline]
pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let rflags: u64;
    // SAFETY: PUSHFQ/POP命令でRFLAGSレジスタを読み取り、CLI命令で割り込みを無効化する。
    // これらはRing 0で実行される特権命令であり、カーネルモードで動作している。
    // RFLAGSの読み取りとCLI命令はメモリアクセスを伴わない。
    unsafe {
        // RFLAGSを保存して割り込みを無効化
        asm!("pushfq; pop {}; cli", out(reg) rflags, options(nomem, nostack));
    }

    let result = f();

    // 元々割り込みが有効だった場合のみ復元
    if rflags & 0x200 != 0 {
        // SAFETY: STI命令はRing 0で実行される特権命令。
        // 元々割り込みが有効だった場合にのみ呼ばれ、
        // IDT/APICは初期化済みなので割り込みを受け付けられる。
        unsafe {
            asm!("sti", options(nomem, nostack));
        }
    }

    result
}

// I/Oポートに1バイト書き込み
#[inline]
pub unsafe fn port_write_u8(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack));
    }
}

// I/Oポートから1バイト読み込み
#[inline]
pub unsafe fn port_read_u8(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack));
    }
    value
}

// I/Oポートに2バイト書き込み
#[inline]
pub unsafe fn port_write_u16(port: u16, value: u16) {
    unsafe {
        asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack));
    }
}

// I/Oポートから2バイト読み込み
#[inline]
pub unsafe fn port_read_u16(port: u16) -> u16 {
    let value: u16;
    unsafe {
        asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack));
    }
    value
}

// I/Oポートに4バイト書き込み
#[inline]
pub unsafe fn port_write_u32(port: u16, value: u32) {
    unsafe {
        asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack));
    }
}

// I/Oポートから4バイト読み込み
#[inline]
pub unsafe fn port_read_u32(port: u16) -> u32 {
    let value: u32;
    unsafe {
        asm!("in eax, dx", in("dx") port, out("eax") value, options(nomem, nostack));
    }
    value
}
