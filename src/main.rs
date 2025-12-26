#![no_main]

use core::arch::asm;

mod uefi;
mod graphics;

use uefi::*;
use graphics::{draw_string};

fn hlt() {
    unsafe {
        asm!("hlt");
    }
}

#[unsafe(no_mangle)]
extern "efiapi" fn efi_main(
    image_handle: EfiHandle,
    system_table: *mut EfiSystemTable,
) -> EfiStatus {
    unsafe {
        // ブートサービスを取得
        let boot_services = (*system_table).boot_services;

        // Graphics Output Protocol を検索
        let mut gop: *mut EfiGraphicsOutputProtocol = core::ptr::null_mut();
        let status = ((*boot_services).locate_protocol)(
            &EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID,
            core::ptr::null_mut(),
            &mut gop as *mut *mut _ as *mut *mut core::ffi::c_void,
        );

        if status != EFI_SUCCESS {
            // GOPが見つからなかった場合は停止
            loop {
                hlt()
            }
        }

        // フレームバッファ情報を取得
        let mode = (*gop).mode;
        let mode_info = (*mode).info;
        let fb_base = (*mode).frame_buffer_base;
        let fb_size = (*mode).frame_buffer_size;
        let width = (*mode_info).horizontal_resolution;
        let _height = (*mode_info).vertical_resolution;

        // フレームバッファを白で塗りつぶす（0xFFFFFFFF = 白）
        let fb_ptr = fb_base as *mut u32;
        let pixel_count = fb_size / 4; // 4バイト = 1ピクセル

        for i in 0..pixel_count {
            *fb_ptr.add(i) = 0xFFFFFFFF;
        }

        // テキストを描画（黒文字）
        draw_string(fb_base, width, 10, 10, "Hello, je4OS!", 0x00000000);
        draw_string(fb_base, width, 10, 30, "Exiting UEFI Boot Services...", 0x00000000);

        // メモリマップを取得
        let mut map_size: usize = 0;
        let mut map_key: usize = 0;
        let mut descriptor_size: usize = 0;
        let mut descriptor_version: u32 = 0;

        // 最初の呼び出しでマップサイズを取得
        ((*boot_services).get_memory_map)(
            &mut map_size,
            core::ptr::null_mut(),
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        );

        // UEFI Boot Services を終了
        let status = ((*boot_services).exit_boot_services)(image_handle, map_key);

        if status == EFI_SUCCESS {
            // ブートサービス終了成功
            draw_string(fb_base, width, 10, 50, "Boot Services Exited!", 0x00000000);
            draw_string(fb_base, width, 10, 70, "Running as OS kernel!", 0x00000000);
        } else {
            // 失敗した場合
            draw_string(fb_base, width, 10, 50, "Exit failed!", 0xFF0000);
        }
    }

    loop {
        hlt()
    }
}
