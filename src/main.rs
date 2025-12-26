#![no_main]

extern crate alloc;

use core::arch::asm;
use core::fmt::Write;

mod uefi;
mod graphics;
mod io;
mod serial;
mod allocator;

#[cfg(feature = "visualize")]
mod visualization;

use uefi::*;
use graphics::FramebufferWriter;

fn hlt() {
    unsafe {
        asm!("hlt");
    }
}

// 待機関数（簡易版）
fn wait_cycles(cycles: usize) {
    for _ in 0..cycles {
        unsafe {
            core::arch::asm!("nop", options(nomem, nostack));
        }
    }
}

// メモリタイプを文字列に変換
fn memory_type_str(mem_type: u32) -> &'static str {
    match mem_type {
        EFI_RESERVED_MEMORY_TYPE => "Reserved",
        EFI_LOADER_CODE => "LoaderCode",
        EFI_LOADER_DATA => "LoaderData",
        EFI_BOOT_SERVICES_CODE => "BSCode",
        EFI_BOOT_SERVICES_DATA => "BSData",
        EFI_RUNTIME_SERVICES_CODE => "RTCode",
        EFI_RUNTIME_SERVICES_DATA => "RTData",
        EFI_CONVENTIONAL_MEMORY => "Available",
        EFI_UNUSABLE_MEMORY => "Unusable",
        EFI_ACPI_RECLAIM_MEMORY => "ACPIReclaim",
        EFI_ACPI_MEMORY_NVS => "ACPINVS",
        EFI_MEMORY_MAPPED_IO => "MMIO",
        EFI_MEMORY_MAPPED_IO_PORT_SPACE => "MMIOPort",
        EFI_PAL_CODE => "PALCode",
        _ => "Unknown",
    }
}

#[unsafe(no_mangle)]
extern "efiapi" fn efi_main(
    image_handle: EfiHandle,
    system_table: *mut EfiSystemTable,
) -> EfiStatus {
    // シリアルポートを初期化
    serial::init();
    println!("=== je4OS Bootloader ===");
    info!("Serial output initialized");
    info!("Locating Graphics Output Protocol...");

    // SAFETY: system_table は UEFI から渡される有効なポインタ
    let boot_services = unsafe { (*system_table).boot_services };

    // Graphics Output Protocol を検索
    let mut gop: *mut EfiGraphicsOutputProtocol = core::ptr::null_mut();

    // SAFETY: UEFI 関数の呼び出し
    let status = unsafe {
        ((*boot_services).locate_protocol)(
            &EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
            core::ptr::null_mut(),
            &mut gop as *mut *mut _ as *mut *mut core::ffi::c_void,
        )
    };

    if status != EFI_SUCCESS {
        error!("Failed to locate GOP!");
        loop {
            hlt()
        }
    }

    info!("GOP found successfully");

    // SAFETY: GOP から有効なフレームバッファ情報を取得
    let (fb_base, fb_size, width, height) = unsafe {
        let mode = (*gop).mode;
        let mode_info = (*mode).info;
        (
            (*mode).frame_buffer_base,
            (*mode).frame_buffer_size,
            (*mode_info).horizontal_resolution,
            (*mode_info).vertical_resolution,
        )
    };

    // SAFETY: フレームバッファへの直接書き込み
    unsafe {
        let fb_ptr = fb_base as *mut u32;
        let pixel_count = fb_size / 4;
        for i in 0..pixel_count {
            *fb_ptr.add(i) = 0x00000000;
        }
    }

    // FramebufferWriter を作成
    let mut writer = FramebufferWriter::new(fb_base, width, height, 0xFFFFFFFF);
    writer.set_position(10, 10);

    // writeln! マクロでテキストを描画
    let _ = writeln!(writer, "je4OS - Memory Map");

    // メモリマップを取得
    let mut map_size: usize = 0;
    let mut map_key: usize = 0;
    let mut descriptor_size: usize = 0;
    let mut descriptor_version: u32 = 0;

    // SAFETY: UEFI 関数呼び出し - メモリマップサイズ取得
    unsafe {
        ((*boot_services).get_memory_map)(
            &mut map_size,
            core::ptr::null_mut(),
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        );
    }

    // バッファを確保（スタック上に）
    let mut buffer = [0u8; 4096 * 4];
    map_size = buffer.len();

    // SAFETY: UEFI 関数呼び出し - 実際のメモリマップ取得
    let status = unsafe {
        ((*boot_services).get_memory_map)(
            &mut map_size,
            buffer.as_mut_ptr() as *mut EfiMemoryDescriptor,
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        )
    };

    if status == EFI_SUCCESS {
        let entry_count = map_size / descriptor_size;
        info!("Memory map retrieved: {} entries", entry_count);

        // メモリマップを表示
        writer.set_position(10, 30);
        let max_display = 20;

        println!("\nMemory Map (first {} entries):", max_display.min(entry_count));
        for i in 0..entry_count.min(max_display) {
            let offset = i * descriptor_size;

            // SAFETY: バッファ内の有効なメモリディスクリプタを参照
            let desc = unsafe {
                &*(buffer.as_ptr().add(offset) as *const EfiMemoryDescriptor)
            };

            let type_str = memory_type_str(desc.r#type);
            println!("  {:<12} 0x{:016X}  Pages: 0x{:X}",
                type_str,
                desc.physical_start,
                desc.number_of_pages
            );

            let _ = writeln!(
                writer,
                "{:<12} 0x{:016X}  Pages: 0x{:X}",
                type_str,
                desc.physical_start,
                desc.number_of_pages
            );
        }

        let _ = writeln!(writer, "");
        let _ = writeln!(writer, "Total entries: {}", entry_count);

        // メモリマップから利用可能なメモリを見つけてアロケータを初期化
        let mut largest_start = 0;
        let mut largest_size = 0;

        for i in 0..entry_count {
            let offset = i * descriptor_size;
            let desc = unsafe {
                &*(buffer.as_ptr().add(offset) as *const EfiMemoryDescriptor)
            };

            // EFI_CONVENTIONAL_MEMORY（利用可能なメモリ）を探す
            if desc.r#type == EFI_CONVENTIONAL_MEMORY {
                let size = desc.number_of_pages * 4096; // 1ページ = 4KB
                if size > largest_size {
                    largest_start = desc.physical_start as usize;
                    largest_size = size;
                }
            }
        }

        if largest_size > 0 {
            // ヒープとして使用するサイズ
            #[cfg(feature = "visualize")]
            let heap_size = (largest_size as usize).min(256 * 1024); // 可視化のため256KBに制限

            #[cfg(not(feature = "visualize"))]
            let heap_size = largest_size as usize; // 本番環境では全て使用

            unsafe {
                allocator::init_heap(largest_start, heap_size);
            }
        } else {
            error!("No usable memory found!");
        }
    }

    // SAFETY: UEFI 関数呼び出し - ブートサービス終了
    info!("Exiting boot services...");
    let status = unsafe {
        ((*boot_services).exit_boot_services)(image_handle, map_key)
    };

    writer.set_position(10, 280);
    if status == EFI_SUCCESS {
        info!("Boot services exited successfully!");
        let _ = writeln!(writer, "Boot Services Exited!");
        writer.set_position(10, 300);

        #[cfg(feature = "visualize")]
        {
            use visualization::*;

            // スラブアロケータの可視化（複数サイズクラス表示）
            let _ = writeln!(writer, "=== Memory Allocator Visualization ===");

            // 初期状態を表示
            draw_code_snippet(&mut writer, &[
                "// Initial state",
                "// No allocations yet",
            ]);
            draw_memory_grids_multi(&mut writer, "Initial State");
        wait_cycles(150_000_000);

        // テスト1: 16Bクラス
        info!("\n=== Test 1: Vec<u8> (16B class) ===");

        let vec1: alloc::vec::Vec<u8> = (0..12).collect();

        draw_code_snippet(&mut writer, &[
            "let vec1: Vec<u8>",
            "  = (0..12).collect();",
            "",
            "// 12 x u8 = 12B",
            "// -> 16B size class",
        ]);
        draw_memory_grids_multi(&mut writer, "After 16B alloc");
        info!("Allocated Vec<u8> (12 elements = 12B -> 16B)");
        wait_cycles(150_000_000);

        // テスト2: 64Bクラス
        info!("\n=== Test 2: Vec<u8> (64B class) ===");

        let vec2: alloc::vec::Vec<u8> = (0..50).collect();

        draw_code_snippet(&mut writer, &[
            "let vec2: Vec<u8>",
            "  = (0..50).collect();",
            "",
            "// 50 x u8 = 50B",
            "// -> 64B size class",
        ]);
        draw_memory_grids_multi(&mut writer, "After 16B + 64B");
        info!("Allocated Vec<u8> (50 elements = 50B -> 64B)");
        wait_cycles(150_000_000);

        // テスト3: 128Bクラス
        info!("\n=== Test 3: Vec<u64> (128B class) ===");

        let vec3: alloc::vec::Vec<u64> = (0..10).collect();

        draw_code_snippet(&mut writer, &[
            "let vec3: Vec<u64>",
            "  = (0..10).collect();",
            "",
            "// 10 x u64 = 80B",
            "// -> 128B size class",
        ]);
        draw_memory_grids_multi(&mut writer, "16B+64B+128B");
        info!("Allocated Vec<u64> (10 elements = 80B -> 128B)");
        wait_cycles(150_000_000);

        // テスト4: 256Bクラスを追加
        info!("\n=== Test 4: Vec<u64> (256B class) ===");

        let vec4: alloc::vec::Vec<u64> = (0..25).collect();

        draw_code_snippet(&mut writer, &[
            "let vec4: Vec<u64>",
            "  = (0..25).collect();",
            "",
            "// 25 x u64 = 200B",
            "// -> 256B size class",
        ]);
        draw_memory_grids_multi(&mut writer, "All 4 sizes");
        info!("Allocated Vec<u64> (25 elements = 200B -> 256B)");
        wait_cycles(150_000_000);

        // テスト5: 64Bと256Bを解放
        info!("\n=== Test 5: Free 64B and 256B ===");

        drop(vec2);
        drop(vec4);

        draw_code_snippet(&mut writer, &[
            "drop(vec2);",
            "drop(vec4);",
            "",
            "// Freed 64B and 256B",
            "// 16B + 128B remain",
        ]);
        draw_memory_grids_multi(&mut writer, "After freeing 2");
        info!("Freed 64B and 256B blocks");
        wait_cycles(150_000_000);

        // テスト6: 全て解放
        info!("\n=== Test 6: Free all ===");

        drop(vec1);
        drop(vec3);

        draw_code_snippet(&mut writer, &[
            "drop(vec1);",
            "drop(vec3);",
            "",
            "// All freed!",
        ]);
        draw_memory_grids_multi(&mut writer, "All freed");
        info!("All blocks freed");
        wait_cycles(150_000_000);
        } // end of visualize cfg

    } else {
        error!("Failed to exit boot services! Status: 0x{:X}", status);
        writer.set_color(0xFF0000); // 赤色
        let _ = writeln!(writer, "Exit failed!");
    }

    println!("\nHalting system...");

    loop {
        hlt()
    }
}
