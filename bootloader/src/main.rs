#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;
use vitros_common::boot_info::{BootInfo, FramebufferInfo, MemoryRegion};
use vitros_common::elf::{Elf64Header, Elf64ProgramHeader, PT_LOAD};
use vitros_common::uefi::*;

// BOOT_INFOをカーネル直前の固定アドレスに配置
// 0x90000 (576KB) - カーネル(0x100000=1MB)の手前で安全
// この領域はConventional Memoryで、ExitBootServices後も有効
const BOOT_INFO_ADDR: usize = 0x90000;

// グローバルなConOut（初期化後に設定）
static mut CON_OUT: Option<*mut EfiSimpleTextOutputProtocol> = None;

// ConOutに文字列を出力するヘルパー関数
fn print_con(s: &str) {
    unsafe {
        if let Some(con_out) = CON_OUT {
            let mut buffer = [0u16; 256];
            let mut len = 0;
            for c in s.chars() {
                if len >= buffer.len() - 1 {
                    break;
                }
                buffer[len] = c as u16;
                len += 1;
            }
            buffer[len] = 0; // null terminator
            ((*con_out).output_string)(con_out, buffer.as_ptr());
        }
    }
}

// 改行付き出力
fn println_con(s: &str) {
    print_con(s);
    print_con("\r\n");
}

// 固定サイズバッファを使ったフォーマット出力
struct BufWriter {
    buf: [u8; 512],
    pos: usize,
}

impl BufWriter {
    fn new() -> Self {
        Self {
            buf: [0; 512],
            pos: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.pos]).unwrap_or("")
    }
}

impl Write for BufWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remaining = self.buf.len() - self.pos;
        let to_write = bytes.len().min(remaining);
        self.buf[self.pos..self.pos + to_write].copy_from_slice(&bytes[..to_write]);
        self.pos += to_write;
        Ok(())
    }
}

// マクロライクなヘルパー
macro_rules! print_uefi {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut buf = BufWriter::new();
        let _ = write!(buf, $($arg)*);
        print_con(buf.as_str());
    }};
}

macro_rules! println_uefi {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let mut buf = BufWriter::new();
        let _ = write!(buf, $($arg)*);
        println_con(buf.as_str());
    }};
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println_con("\n!!! BOOTLOADER PANIC !!!");
    println_uefi!("{}", info);
    loop {
        unsafe { core::arch::asm!("hlt") }
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

// ページテーブルエントリのフラグ
const PAGE_PRESENT: u64 = 1 << 0;
const PAGE_WRITABLE: u64 = 1 << 1;
const PAGE_HUGE: u64 = 1 << 7;

// カーネル仮想アドレスベース
const KERNEL_VMA: u64 = 0xFFFF800000000000;

// ページテーブル構造体（4KBアラインメント）
#[repr(C, align(4096))]
struct PageTable {
    entries: [u64; 512],
}

impl PageTable {
    const fn new() -> Self {
        Self { entries: [0; 512] }
    }
}

// グローバルページテーブル（静的に確保）
static mut BOOT_PML4: PageTable = PageTable::new();
static mut BOOT_PDP_LOW: PageTable = PageTable::new();
static mut BOOT_PDP_HIGH: PageTable = PageTable::new();
static mut BOOT_PD_LOW: [PageTable; 8] = [
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
];
static mut BOOT_PD_HIGH: [PageTable; 8] = [
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
    PageTable::new(),
];

/// ブートローダー用の初期ページテーブルをセットアップ
unsafe fn setup_initial_page_tables() -> u64 {
    let flags = PAGE_PRESENT | PAGE_WRITABLE;
    let huge_flags = flags | PAGE_HUGE;

    unsafe {
        // PML4[0] -> PDP_LOW (低位アドレス: 0x0-0x7FFFFFFFFF)
        BOOT_PML4.entries[0] = &raw const BOOT_PDP_LOW as u64 | flags;

        // PML4[256] -> PDP_HIGH (高位アドレス: 0xFFFF800000000000-)
        BOOT_PML4.entries[256] = &raw const BOOT_PDP_HIGH as u64 | flags;

        // 低位: 最初の8GBをアイデンティティマッピング
        for i in 0..8 {
            BOOT_PDP_LOW.entries[i] = &raw const BOOT_PD_LOW[i] as u64 | flags;

            for j in 0..512 {
                let phys_addr = ((i * 512 + j) * 2 * 1024 * 1024) as u64;
                BOOT_PD_LOW[i].entries[j] = phys_addr | huge_flags;
            }
        }

        // 高位: 最初の8GBを0xFFFF800000000000+にマッピング
        for i in 0..8 {
            BOOT_PDP_HIGH.entries[i] = &raw const BOOT_PD_HIGH[i] as u64 | flags;

            for j in 0..512 {
                let phys_addr = ((i * 512 + j) * 2 * 1024 * 1024) as u64;
                BOOT_PD_HIGH[i].entries[j] = phys_addr | huge_flags;
            }
        }

        // PML4のアドレスを返す
        &raw const BOOT_PML4 as u64
    }
}

/// CR3にページテーブルをロードしてページングを有効化（既に有効なのでCR3のみ更新）
unsafe fn load_page_tables(pml4_addr: u64) {
    unsafe {
        core::arch::asm!(
            "mov cr3, {0}",
            in(reg) pml4_addr,
            options(nostack, preserves_flags)
        );
    }
}

/// UEFI エントリポイント
#[unsafe(no_mangle)]
extern "efiapi" fn efi_main(
    image_handle: EfiHandle,
    system_table: *mut EfiSystemTable,
) -> EfiStatus {
    // ConOut (UEFI Simple Text Output Protocol) を初期化
    unsafe {
        CON_OUT = Some((*system_table).con_out);
    }

    println_con("=== VitrOS Bootloader ===");
    println_uefi!("[INFO] UEFI ConOut initialized");
    println_uefi!("[INFO] Locating Graphics Output Protocol...");

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
        println_uefi!("[ERROR] Failed to locate GOP!");
        loop {
            unsafe { core::arch::asm!("hlt") }
        }
    }

    println_uefi!("[INFO] GOP found successfully");

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

    // 画面クリア（ConOut使用）
    unsafe {
        if let Some(con_out) = CON_OUT {
            ((*con_out).clear_screen)(con_out);
        }
    }

    println_uefi!("\nVitrOS - Memory Map\n");

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

    // BOOT_INFOを固定アドレス（0x90000）に配置
    let boot_info = unsafe { &mut *(BOOT_INFO_ADDR as *mut BootInfo) };
    *boot_info = BootInfo::new();

    // フレームバッファ情報を設定
    boot_info.framebuffer = FramebufferInfo {
        base: fb_base,
        size: fb_size as u64,
        width,
        height,
        stride: width,
    };

    // RSDP (ACPI Root System Description Pointer) を UEFI Configuration Table から取得
    unsafe {
        let config_table_ptr = (*system_table).configuration_table as *const EfiConfigurationTable;
        let num_entries = (*system_table).number_of_table_entries;

        let mut rsdp_addr = 0u64;
        for i in 0..num_entries {
            let entry = &*config_table_ptr.add(i);

            // ACPI 2.0 を優先的に検索
            if entry.vendor_guid == EFI_ACPI_20_TABLE_GUID {
                rsdp_addr = entry.vendor_table;
                println_uefi!("[INFO] Found ACPI 2.0 RSDP at 0x{:016X}", rsdp_addr);
                break;
            }
            // ACPI 1.0 をフォールバック
            else if entry.vendor_guid == EFI_ACPI_TABLE_GUID {
                rsdp_addr = entry.vendor_table;
                println_uefi!("[INFO] Found ACPI 1.0 RSDP at 0x{:016X}", rsdp_addr);
            }
        }

        if rsdp_addr == 0 {
            println_uefi!("[INFO] RSDP not found in UEFI Configuration Table");
        }

        boot_info.rsdp_address = rsdp_addr;
    }

    if status == EFI_SUCCESS {
        let entry_count = map_size / descriptor_size;
        println_uefi!("[INFO] Memory map retrieved: {} entries", entry_count);

        // メモリマップを表示
        let max_display = 20;

        println_uefi!(
            "\nMemory Map (first {} entries):",
            max_display.min(entry_count)
        );
        for i in 0..entry_count.min(max_display) {
            let offset = i * descriptor_size;

            // SAFETY: バッファ内の有効なメモリディスクリプタを参照
            let desc = unsafe { &*(buffer.as_ptr().add(offset) as *const EfiMemoryDescriptor) };

            let type_str = memory_type_str(desc.r#type);
            println_uefi!(
                "  {:<12} 0x{:016X}  Pages: 0x{:X}",
                type_str,
                desc.physical_start,
                desc.number_of_pages
            );
        }

        println_uefi!("\nTotal entries: {}", entry_count);

        // BootInfo にメモリマップをコピー
        for i in 0..entry_count.min(boot_info.memory_map.len()) {
            let offset = i * descriptor_size;
            let desc = unsafe { &*(buffer.as_ptr().add(offset) as *const EfiMemoryDescriptor) };

            boot_info.memory_map[i] = MemoryRegion {
                start: desc.physical_start,
                size: desc.number_of_pages * 4096,
                region_type: desc.r#type,
            };
        }
        boot_info.memory_map_count = entry_count.min(boot_info.memory_map.len());
        println_uefi!("[INFO] BOOT_INFO at 0x{:X}", BOOT_INFO_ADDR);
        println_uefi!(
            "[INFO] BOOT_INFO.memory_map_count = {}",
            boot_info.memory_map_count
        );
        println_uefi!(
            "[INFO] BOOT_INFO.memory_map[0]: start=0x{:X}, size=0x{:X}, type={}",
            boot_info.memory_map[0].start,
            boot_info.memory_map[0].size,
            boot_info.memory_map[0].region_type
        );
    }

    // カーネルをロード (ブートサービス終了前に実行)
    println_uefi!("[INFO] Loading kernel from ELF...");
    let kernel_entry = load_kernel_elf(image_handle, boot_services);
    if kernel_entry == 0 {
        println_uefi!("[ERROR] Failed to load kernel!");
        loop {
            unsafe { core::arch::asm!("hlt") }
        }
    }
    println_uefi!("[INFO] Kernel entry point: 0x{:X}", kernel_entry);

    // カーネルロード後にメモリマップが変更されているので、再取得
    println_uefi!("[INFO] Updating memory map before ExitBootServices...");

    // まず必要なサイズを取得
    map_size = 0;
    unsafe {
        ((*boot_services).get_memory_map)(
            &mut map_size,
            core::ptr::null_mut(),
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        );
    }

    map_size += descriptor_size;

    if map_size > buffer.len() {
        println_uefi!(
            "[ERROR] Memory map too large! Required: {}, Available: {}",
            map_size,
            buffer.len()
        );
        loop {
            unsafe { core::arch::asm!("hlt") }
        }
    }

    let status = unsafe {
        ((*boot_services).get_memory_map)(
            &mut map_size,
            buffer.as_mut_ptr() as *mut EfiMemoryDescriptor,
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        )
    };
    if status != EFI_SUCCESS {
        println_uefi!(
            "[ERROR] Failed to get updated memory map! Status: 0x{:X}",
            status
        );
        loop {
            unsafe { core::arch::asm!("hlt") }
        }
    }

    // SAFETY: UEFI 関数呼び出し - ブートサービス終了
    // GetMemoryMap後はBoot Serviceを使用しない（MapKeyが無効になるため）
    let status = unsafe { ((*boot_services).exit_boot_services)(image_handle, map_key) };

    if status != EFI_SUCCESS {
        // ExitBootServicesが失敗した場合は、まだBootServicesが有効なのでConOutが使える
        println_uefi!(
            "[ERROR] Failed to exit boot services! Status: 0x{:X}",
            status
        );
        loop {
            unsafe { core::arch::asm!("hlt") }
        }
    }

    // ExitBootServices成功 - ここから先はBoot Servicesは使用不可

    // ページテーブルをセットアップ
    let pml4_addr = unsafe { setup_initial_page_tables() };

    // CR3にページテーブルをロード
    unsafe { load_page_tables(pml4_addr) };

    // カーネルジャンプ直前にBOOT_INFOを再確認
    let boot_info_check = unsafe { &*(BOOT_INFO_ADDR as *const BootInfo) };

    // カーネルの高位仮想アドレスを計算（kernel_entryは物理アドレス）
    let kernel_high_addr = kernel_entry + KERNEL_VMA;

    // カーネルにジャンプ (efiapi calling convention to match kernel entry point)
    type KernelEntry = extern "efiapi" fn(&'static BootInfo) -> !;
    let kernel_fn: KernelEntry = unsafe { core::mem::transmute(kernel_high_addr as *const ()) };
    kernel_fn(boot_info_check);
}

/// ELFファイルからカーネルをロード
fn load_kernel_elf(_image_handle: EfiHandle, boot_services: *mut EfiBootServices) -> u64 {
    // Simple File System Protocolを直接検索
    let mut sfs: *mut EfiSimpleFileSystemProtocol = core::ptr::null_mut();
    let status = unsafe {
        ((*boot_services).locate_protocol)(
            &EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID,
            core::ptr::null_mut(),
            &mut sfs as *mut *mut _ as *mut *mut core::ffi::c_void,
        )
    };
    if status != EFI_SUCCESS {
        println_uefi!("[ERROR] Failed to locate Simple File System Protocol");
        return 0;
    }

    // ルートディレクトリを開く
    let mut root: *mut EfiFileProtocol = core::ptr::null_mut();
    let status = unsafe { ((*sfs).open_volume)(sfs, &mut root) };
    if status != EFI_SUCCESS {
        println_uefi!("[ERROR] Failed to open root volume");
        return 0;
    }

    // kernel.elfを開く
    let kernel_name = to_utf16("kernel.elf");
    let mut kernel_file: *mut EfiFileProtocol = core::ptr::null_mut();
    let status = unsafe {
        ((*root).open)(
            root,
            &mut kernel_file,
            kernel_name.as_ptr(),
            EFI_FILE_MODE_READ,
            0,
        )
    };
    if status != EFI_SUCCESS {
        println_uefi!("[ERROR] Failed to open kernel.elf");
        return 0;
    }

    // ファイルを一時バッファに読み込む (最大2MB - staticを使用)
    static mut FILE_BUFFER: [u8; 2 * 1024 * 1024] = [0; 2 * 1024 * 1024];
    let file_buffer = unsafe { &mut *core::ptr::addr_of_mut!(FILE_BUFFER) };
    let mut file_size = file_buffer.len();
    let status = unsafe {
        ((*kernel_file).read)(
            kernel_file,
            &mut file_size,
            file_buffer.as_mut_ptr() as *mut core::ffi::c_void,
        )
    };
    unsafe {
        ((*kernel_file).close)(kernel_file);
        ((*root).close)(root);
    }

    if status != EFI_SUCCESS {
        println_uefi!("[ERROR] Failed to read kernel file");
        return 0;
    }

    println_uefi!("[INFO] Kernel loaded: {} bytes", file_size);

    // ELFヘッダーを検証
    let elf_header = unsafe { &*(file_buffer.as_ptr() as *const Elf64Header) };
    if !elf_header.is_valid() {
        println_uefi!("[ERROR] Invalid ELF header");
        return 0;
    }

    // プログラムヘッダーを処理してLOADセグメントをメモリにコピー
    // 最初のLOADセグメントから仮想/物理アドレスのオフセットを計算
    let mut kernel_virt_offset: Option<u64> = None;

    for i in 0..elf_header.e_phnum {
        let ph_offset =
            elf_header.e_phoff as usize + (i as usize * core::mem::size_of::<Elf64ProgramHeader>());
        let ph = unsafe { &*(file_buffer.as_ptr().add(ph_offset) as *const Elf64ProgramHeader) };

        if ph.p_type == PT_LOAD {
            // 最初のLOADセグメントから仮想/物理アドレスのオフセットを記録
            if kernel_virt_offset.is_none() && ph.p_vaddr != ph.p_paddr {
                kernel_virt_offset = Some(ph.p_vaddr - ph.p_paddr);
            }

            // ファイルからメモリにコピー
            unsafe {
                let src = file_buffer.as_ptr().add(ph.p_offset as usize);
                let dst = ph.p_paddr as *mut u8;
                core::ptr::copy_nonoverlapping(src, dst, ph.p_filesz as usize);

                // 残りをゼロクリア (BSS領域)
                if ph.p_memsz > ph.p_filesz {
                    core::ptr::write_bytes(
                        dst.add(ph.p_filesz as usize),
                        0,
                        (ph.p_memsz - ph.p_filesz) as usize,
                    );
                }
            }
        }
    }

    // エントリポイントを物理アドレスに変換
    // カーネルが高位アドレスでリンクされている場合、仮想アドレスを物理アドレスに変換
    let physical_entry = if let Some(offset) = kernel_virt_offset {
        elf_header.e_entry - offset
    } else {
        elf_header.e_entry
    };

    physical_entry
}

/// 文字列をUTF-16に変換
fn to_utf16(s: &str) -> [u16; 32] {
    let mut buf = [0u16; 32];
    for (i, c) in s.chars().enumerate() {
        if i >= 31 {
            break;
        }
        buf[i] = c as u16;
    }
    buf
}
