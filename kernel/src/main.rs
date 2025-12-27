#![no_std]
#![no_main]

extern crate alloc;

// OS カーネル処理
// アロケータ初期化、可視化テスト、メインループ

use je4os_common::boot_info::BootInfo;
use je4os_common::graphics::FramebufferWriter;
use je4os_common::{allocator, error, info, println, uefi};
use core::arch::asm;
use core::fmt::Write;
use core::panic::PanicInfo;

#[cfg(feature = "visualize-allocator")]
use je4os_common::allocator_visualization;

// パニックハンドラ
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n!!! KERNEL PANIC !!!");
    println!("{}", info);
    loop {
        hlt()
    }
}

fn hlt() {
    unsafe {
        asm!("hlt");
    }
}

/// カーネルエントリポイント
/// ブートローダから呼ばれる
#[unsafe(no_mangle)]
extern "C" fn kernel_main(boot_info: &'static BootInfo) -> ! {
    info!("=== Kernel Started ===");

    // フレームバッファライターを作成
    let mut writer = FramebufferWriter::new(
        boot_info.framebuffer.base,
        boot_info.framebuffer.width,
        boot_info.framebuffer.height,
        0xFFFFFFFF,
    );
    writer.set_position(10, 300);

    // boot_info の情報はFramebufferWriterで表示（こちらは安全）
    let _ = writeln!(
        writer,
        "Framebuffer: 0x{:X}, {}x{}",
        boot_info.framebuffer.base, boot_info.framebuffer.width, boot_info.framebuffer.height
    );
    let _ = writeln!(writer, "Memory regions: {}", boot_info.memory_map_count);

    // 利用可能なメモリを探してアロケータを初期化
    let mut largest_start = 0;
    let mut largest_size = 0;

    for i in 0..boot_info.memory_map_count {
        let region = &boot_info.memory_map[i];
        // region_type == 7 は EFI_CONVENTIONAL_MEMORY
        if region.region_type == uefi::EFI_CONVENTIONAL_MEMORY && region.size > largest_size as u64
        {
            largest_start = region.start as usize;
            largest_size = region.size as usize;
        }
    }

    if largest_size > 0 {
        info!("Found usable memory");
        let _ = writeln!(
            writer,
            "Largest usable memory: 0x{:X} - 0x{:X} ({} MB)",
            largest_start,
            largest_start + largest_size,
            largest_size / 1024 / 1024
        );

        // ヒープサイズを決定
        #[cfg(feature = "visualize-allocator")]
        let heap_size = largest_size.min(256 * 1024); // 可視化のため256KBに制限

        #[cfg(not(feature = "visualize-allocator"))]
        let heap_size = largest_size; // 本番環境では全て使用

        unsafe {
            allocator::init_heap(largest_start, heap_size);
        }

        let _ = writeln!(writer, "Heap initialized: {} KB", heap_size / 1024);
        info!("Heap initialized successfully");
    } else {
        error!("No usable memory found!");
        let _ = writeln!(writer, "ERROR: No usable memory!");
    }

    // 可視化テストを実行
    #[cfg(feature = "visualize-allocator")]
    {
        info!("Starting allocator visualization");
        allocator_visualization::run_visualization_tests(&mut writer);
    }

    #[cfg(not(feature = "visualize-allocator"))]
    {
        let _ = writeln!(writer, "");
        let _ = writeln!(writer, "Kernel running...");
        let _ = writeln!(writer, "System ready.");
    }

    info!("Entering main loop");

    // メインループ
    loop {
        hlt()
    }
}
