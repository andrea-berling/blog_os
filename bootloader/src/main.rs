// Bless be thee:
// https://stackoverflow.com/questions/67902309/how-to-compile-rust-code-to-bare-metal-32-bit-x86-i686-code-what-compile-targ#67902310
#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(clippy::undocumented_unsafe_blocks)]

use core::arch::{asm, naked_asm};

mod edd;
mod error;

use error::Facility::Bootloader;

#[cfg(target_os = "none")]
use core::panic::PanicInfo;

use common::{
    ata,
    control_registers::{
        self, ControlRegister0, ControlRegister3, ControlRegister4, ExtendedFeatureEnableRegister,
    },
    elf::{self, program_header::ProgramHeaderEntryType},
    error::{Context, InternalError, Kind, Reason},
    gdt::{self, SegmentDescriptor},
    idt,
    paging::{self},
    tss, vga,
};

use crate::edd::DRIVE_PARAMETERS_BUFFER_SIZE;

/// This function is called on panic.
#[cfg(target_os = "none")]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    vga::writeln_no_sync!("{info:#?}").unwrap();
    loop {}
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.start")]
#[cfg(target_os = "none")]
pub extern "cdecl" fn start(
    drive_parameters_pointer: *const u8,
    stage2_sectors: u32,
    kernel_sectors: u32,
    stack_start: u32,
    _edd_version: u32,
    _extensions_bitmap: u32,
) -> ! {
    vga::writeln_no_sync!("Hello from stage2!").unwrap();

    let kernel = load_kernel_from_boot_disk(
        drive_parameters_pointer,
        stage2_sectors,
        kernel_sectors,
        stack_start,
    )
    .inspect_err(|err| vga::writeln_no_sync!("{err:#}").unwrap())
    .unwrap();
    vga::writeln_no_sync!("Read kernel from disk!").unwrap();

    let entrypoint = kernel.header().entrypoint();

    if entrypoint >= u32::MAX as u64 {
        panic!(
            "ERROR: the kernel entrypoint can not be above the 4GB limit of legacy mode, got: {:#x}",
            entrypoint
        );
    }

    // FIXME: what if the size of all statics in the kernel gets larger that 1MB? One should
    // probably find the highest address mapped for the kernel, and add 1MB to that
    // 1MB of stack + heap should be enough for the first stage of the kernel, right?
    let stack_pointer: u32 = entrypoint
        .next_multiple_of(0x100000)
        .try_into()
        .inspect_err(|_| {
            vga::writeln_no_sync!(
                "Kernel entrypoint too high for a stack of 1MB: entrypoint={entrypoint:#x}"
            )
            .unwrap()
        })
        .unwrap();

    load_segments_into_memory(&kernel)
        .inspect_err(|err| vga::writeln_no_sync!("{err:#}").unwrap())
        .unwrap();
    vga::writeln_no_sync!("Loaded kernel segments into memory!").unwrap();

    setup_page_tables()
        .inspect_err(|err| vga::writeln_no_sync!("{err:#}").unwrap())
        .unwrap();

    setup_global_descriptor_table();
    //setup_debug_interrupt_descriptor_table();
    let (cr0, cr3, cr4, efer) = setup_control_registers().unwrap();

    // SAFETY: A valid page table was set up in setup_page_tables, and cr3 was loaded with its
    // address in setup_control_regsiters.
    // cr4 was set up in setup_control_regsiters with the PAE and PSE flags enabled The following
    // assembly is necessary to set the values of the control registers, and because of the reason
    // above is safe
    unsafe {
        asm!(
          "mov cr4, {cr4:e}",
          "mov cr3, {cr3:e}",
          cr4 = in(reg) u32::from(cr4),
          cr3 = in(reg) u64::from(cr3) as u32,
        );
    }

    // SAFETY: This is safe because the EFER register index is being used, and it is being set to
    // efer, which was set up correctly in setup_control_registers to enable IA32e (i.e. long mode)
    unsafe {
        wrmsr(
            control_registers::EXTENDED_FEATURE_ENABLE_REGISTER_MSR_INDEX,
            (u64::from(efer) >> 32) as u32,
            u64::from(efer) as u32,
        );
    }

    // SAFETY: Cr0 was set to enable paging and protected mode
    // The GDT was set up by setup_global_descriptor_table
    // A stack pointer of ~1MB was set up above
    // We need some assembly to set CR0, set the stack, and far jump to the kernel entrypoint, and
    // because of the reasons above, this is safe
    unsafe {
        asm!(
          "mov cr0, {cr0:e}",
          "mov esp, {stack_pointer:e}",
          // Code selector
          "push {code_selector}",
          "push {kernel_entrypoint}",
          "retf",
          cr0 = in(reg) u32::from(cr0),
          out("ax") _,
          kernel_entrypoint = in(reg) kernel.header().entrypoint() as u32,
          stack_pointer = in(reg) stack_pointer,
          code_selector = const GDTI_64_BIT_CODE_SEGMENT * size_of::<gdt::SegmentDescriptor>(),
        )
    }

    panic!("We didn't load the kernel?");
}

#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn wrmsr(msr: u32, high: u32, low: u32) {
    asm!(
      "wrmsr",
      in("eax") low,
      in("edx") high,
      in("ecx") msr,
    )
}

fn setup_control_registers() -> error::Result<(
    ControlRegister0,
    ControlRegister3,
    ControlRegister4,
    ExtendedFeatureEnableRegister,
)> {
    use control_registers::ControlRegister0Bit::*;
    use control_registers::ControlRegister4Bit::*;
    use control_registers::ExtendedFeatureEnableRegisterBit::*;
    let cr0 = ProtectedMode | Paging;
    let mut cr3 = ControlRegister3::empty();
    let cr4: ControlRegister4 = PhysicalAddressExtensions | PhysicalSizeExtensions;
    let efer: ExtendedFeatureEnableRegister = IA32eEnabled.into();

    // SAFETY: This is safe because we are in the bootloader and no other threads are running.
    #[allow(static_mut_refs)]
    cr3.set_pml4(unsafe { &PML4 }).map_err(|reason| {
        common::error::Error::from(InternalError::new(
            Bootloader,
            Kind::CantSetupControlRegisters(reason),
            Context::SettingUpProcessor,
        ))
    })?;

    Ok((cr0, cr3, cr4, efer))
}

static mut GLOBAL_DESCRIPTOR_TABLE: gdt::GDT<6> = [gdt::SegmentDescriptor::blank(); 6];
static mut TASK_STATE_SEGMENT: tss::TaskStateSegment = tss::TaskStateSegment::blank();
static SS0_STACK: tss::Stack<1024> = tss::Stack::new([0; 1024]);

const GDTI_32_BIT_CODE_SEGMENT: usize = 1;
const GDTI_32_BIT_DATA_SEGMENT: usize = 2;
const GDTI_64_BIT_CODE_SEGMENT: usize = 3;
const GDTI_64_BIT_DATA_SEGMENT: usize = 4;
const GDTI_TSS: usize = 5;

fn setup_global_descriptor_table() {
    use gdt::SegmentKind::*;
    macro_rules! update_gdt {
        ($gdt:ident[$gdt_index:expr] => $segment_decriptor:expr) => {
            // SAFETY: This is safe because we are in the bootloader and no other threads are
            // running.
            *(unsafe { &mut $gdt[$gdt_index] }) = $segment_decriptor;
        };
    }
    macro_rules! update_static_mut {
        ($static_mut:expr => $new_val:expr) => {
            let __static_mut_ptr = &raw mut $static_mut;
            // SAFETY: This is safe because we are in the bootloader and no other threads are
            // running.
            *(unsafe { &mut *__static_mut_ptr }) = $new_val;
        };
    }

    update_gdt!(
        GLOBAL_DESCRIPTOR_TABLE[GDTI_32_BIT_CODE_SEGMENT] =>
            SegmentDescriptor::new_flat(Code, false)
    );
    update_gdt!(
        GLOBAL_DESCRIPTOR_TABLE[GDTI_32_BIT_DATA_SEGMENT] =>
            SegmentDescriptor::new_flat(Data, false)
    );
    update_gdt!(
        GLOBAL_DESCRIPTOR_TABLE[GDTI_64_BIT_CODE_SEGMENT] =>
            SegmentDescriptor::new_flat(Code, true)
    );
    update_gdt!(
        GLOBAL_DESCRIPTOR_TABLE[GDTI_64_BIT_DATA_SEGMENT] =>
            SegmentDescriptor::new_flat(Data, true)
    );

    update_static_mut!(TASK_STATE_SEGMENT =>
    tss::TaskStateSegment::with_ss0_stack((GDTI_32_BIT_DATA_SEGMENT * size_of::<gdt::SegmentDescriptor>()).try_into().expect("too big"), &SS0_STACK));

    #[allow(static_mut_refs)]
    // SAFETY: This is safe because we are in the bootloader and no other threads are running.
    let tss = unsafe { &TASK_STATE_SEGMENT };
    update_gdt!(
        GLOBAL_DESCRIPTOR_TABLE[GDTI_TSS] =>
        gdt::SegmentDescriptor::new_tss(tss)
    );

    #[allow(static_mut_refs)]
    // SAFETY: This is safe because we are in the bootloader and no other threads are running.
    let gdt_descriptor = gdt::GDTDescriptor::from(unsafe { &GLOBAL_DESCRIPTOR_TABLE });
    let tss_selector = tss::Selector::with_index(GDTI_TSS as u8);

    // SAFETY: The GDT was set up above with 2 32-bit segments, one for data and one for code, 2
    // 64-bit segments, one for data and one for code, and a TSS for task switching when handling
    // exceptions
    // A GDT descriptor was set in the gdt_descriptor variable pointing to the built up GDT
    // A TSS selector was set in the tss_selector variable pointing to the built up TSS
    // The following assembly is needed to set the GDTR, the Task Segment Status register, and to
    // reload the GDT
    unsafe {
        asm!("lgdt [{gdt_descriptor}]",
             "ltr ax",
             "mov ax, {data_selector}",
             "mov ds, ax",
             "mov es, ax",
             "mov ss, ax",
             "mov fs, ax",
             "mov gs, ax",
             data_selector = const GDTI_64_BIT_DATA_SEGMENT * size_of::<gdt::SegmentDescriptor>(),
             gdt_descriptor = in(reg) &gdt_descriptor,
             in("ax") u8::from(tss_selector) as u16,
        )
    }
}

static mut INTERRUPT_DESCRIPTOR_TABLE: idt::IDT<{ idt::STANDARD_VECTOR_TABLE_SIZE }> =
    [idt::GateDescriptor::blank(); _];

extern "cdecl" fn general_protection_handler(
    ebp: u32,
    edi: u32,
    esi: u32,
    edx: u32,
    ecx: u32,
    ebx: u32,
    eax: u32,
    error_code: u32,
    eip: u32,
    cs: u32,
    eflags: u32,
) {
    let cr2: u32;
    let cr3: u32;

    // SAFETY: This is safe because we are only reading the registers to print them out.
    unsafe {
        asm!("mov {cr2}, cr2", "mov {cr3}, cr3", cr2 = out(reg) cr2, cr3 = out(reg) cr3);
    }

    vga::writeln_no_sync!("General Protection Fault!").unwrap();
    vga::writeln_no_sync!(
        "EAX={:08X} EBX={:08X} ECX={:08X} EDX={:08X}",
        eax,
        ebx,
        ecx,
        edx
    )
    .unwrap();
    vga::writeln_no_sync!("ESI={:08X} EDI={:08X} EBP={:08X}", esi, edi, ebp).unwrap();
    vga::writeln_no_sync!(
        "EIP={:08X} CS={:08X} EFLAGS={:08X} ERROR_CODE={:08X}",
        eip,
        cs,
        eflags,
        error_code
    )
    .unwrap();
    vga::writeln_no_sync!("CR2={:08X} CR3={:08X}", cr2, cr3).unwrap();
    loop {}
}

#[unsafe(naked)]
extern "C" fn general_protection_stub() {
    naked_asm!(
        "push eax", "push ebx", "push ecx", "push edx", "push esi", "push edi", "push ebp",
        "call {handler}",
        "pop ebp", "pop edi", "pop esi", "pop edx", "pop ecx", "pop ebx", "pop eax",
        "add esp, 8",                // discard error_code (we handled it)
        "hlt", handler = sym general_protection_handler,
    );
}

fn setup_debug_interrupt_descriptor_table() {
    let idt_ptr = &raw mut INTERRUPT_DESCRIPTOR_TABLE;
    // SAFETY: This is safe because we are in the bootloader and no other threads are running.
    let gp_descriptor = unsafe { &mut (*idt_ptr)[idt::Interrupt::GeneralProtectionFault as usize] };

    *gp_descriptor = idt::InterruptGateDescriptor::with_address_and_segment_selector(
        general_protection_stub as *const fn() -> () as u32,
        GDTI_32_BIT_CODE_SEGMENT as u16 * size_of::<gdt::SegmentDescriptor>() as u16,
    )
    .into();

    let idt_descriptor = idt::IDTDescriptor::new(
        size_of::<u64>() as u16 * idt::STANDARD_VECTOR_TABLE_SIZE as u16,
        idt_ptr as *const _ as u32,
    );

    // SAFETY: A handler for GP was set up in the global IDT variable
    // A descriptor pointing to the global IDT was correctly created and stored in the
    // idt_descriptor variable
    // The following assembly is necessary to load the IDT, and because of the reasons above is
    // safe
    unsafe {
        asm!("lidt [{idt_descriptor}]",
             idt_descriptor = in(reg)&idt_descriptor
        );
    }
}

static mut PML4: paging::PML4 = paging::PML4::new();
static mut PAGE_DIRECTORY_POINTER_TABLE: paging::PageDirectoryPointerTable =
    paging::PageDirectoryPointerTable::new();

fn setup_page_tables() -> error::Result<()> {
    let pdpt_ptr = &raw mut PAGE_DIRECTORY_POINTER_TABLE;
    // SAFETY: This is safe because we are in the bootloader and no other threads are running.
    let pdpt = unsafe { &mut *pdpt_ptr };

    pdpt.entries[0].set_physical_address(core::ptr::null::<u8>().try_into().map_err(|reason| {
        common::error::Error::from(InternalError::new(
            Bootloader,
            Kind::CantSetupPageTable(reason),
            Context::SettingUpProcessor,
        ))
    })?);
    pdpt.entries[0].set_flag(paging::PageTableEntryFlag::Write);

    let pml4_ptr = &raw mut PML4;
    // SAFETY: This is safe because we are in the bootloader and no other threads are running.
    let pml4 = unsafe { &mut *pml4_ptr };

    // SAFETY: This is safe because we are in the bootloader and no other threads are running.
    pml4.entries[0].set_page_directory_pointer_table(unsafe { &*pdpt_ptr });
    pml4.entries[0].set_flag(paging::PageTableEntryFlag::Write);

    Ok(())
}

fn load_segments_into_memory(kernel: &elf::File<'static>) -> error::Result<()> {
    for loadable_program_header in kernel.program_headers().filter_map(|program_header| {
        program_header.ok().and_then(|program_header| {
            if matches!(program_header.r#type(), ProgramHeaderEntryType::Load) {
                Some(program_header)
            } else {
                None
            }
        })
    }) {
        let loading_address = loadable_program_header.virtual_address();
        let size = loadable_program_header.segment_size_on_file();
        if loading_address <= start as *const () as u64 || loading_address + size >= u32::MAX as u64
        {
            return Err(InternalError::new(
                Bootloader,
                Kind::CantLoadSegment(Reason::InvalidSegmentParameters {
                    virtual_address: loading_address,
                    size,
                }),
                Context::LoadingKernel,
            )
            .into());
        }

        // SAFETY: Virtual address and size have been verified above to be at a address range
        // accessible from 32-bit
        let loading_area = unsafe {
            core::slice::from_raw_parts_mut(
                loadable_program_header.virtual_address() as *mut u8,
                loadable_program_header.segment_size_on_file() as usize,
            )
        };
        loading_area.copy_from_slice(kernel.get_segment(&loadable_program_header).unwrap());
    }
    Ok(())
}

fn load_kernel_from_boot_disk(
    drive_parameters_pointer: *const u8,
    stage2_sectors: u32,
    kernel_sectors: u32,
    stack_start: u32,
) -> Result<elf::File<'static>, error::Error> {
    // SAFETY: The call to BIOS interrupt 13h with AH=48h returned without error in stage1 if we
    // got to stage2, and the drive_parameters_pointer, passed during stage1 to start, points to a
    // buffer of 30 bytes containing the result
    let drive_parameters_bytes = unsafe {
        core::ptr::slice_from_raw_parts(drive_parameters_pointer, DRIVE_PARAMETERS_BUFFER_SIZE)
            .as_ref()
            .unwrap()
    };

    // SAFETY: For the reasons above, it's just as safe to unwrap here
    let drive_parameters = edd::DriveParameters::try_from(drive_parameters_bytes)
        .inspect_err(|err| vga::writeln_no_sync!("{err:#}").unwrap())
        .unwrap();

    match ata::Device::try_from(drive_parameters) {
        Ok(ata_device) => {
            let kernel_size_bytes =
                (kernel_sectors * ata_device.sector_size_bytes() as u32) as usize;
            // SAFETY: The start of the stack for stage 2 and the number of sectors in the kernel were
            // correctly determined at compile time and passed by the stage1
            let kernel_bytes = unsafe {
                core::ptr::slice_from_raw_parts_mut(
                    // Align to a 8 byte boundary (for reading a ELF header)
                    ((stack_start + 7) & !0x7) as *mut u8,
                    kernel_size_bytes,
                )
                .as_mut()
                .unwrap()
            };

            // FIXME: if the kernel gets large enough, we might want to read it in multiple
            // operations, or use lba48
            if kernel_sectors > 256 {
                return Err(InternalError::new(
                    Bootloader,
                    Kind::CantReadKernelFromDisk(Reason::TooManySectors(kernel_sectors)),
                    Context::LoadingKernel,
                )
                .into());
            }
            let Ok(()) = ata_device
                .read_sectors_lba28_pio(kernel_sectors as u8, stage2_sectors + 1, kernel_bytes)
                .map_err(|err| vga::writeln_no_sync!("{err:#}").unwrap())
            else {
                return Err(InternalError::new(
                    Bootloader,
                    Kind::CantReadKernelFromDisk(Reason::IOError),
                    Context::LoadingKernel,
                )
                .into());
            };

            elf::File::try_from(&kernel_bytes[..kernel_size_bytes]).map_err(|err| {
                vga::writeln_no_sync!("{err:#}").unwrap();
                InternalError::new(
                    Bootloader,
                    Kind::CantReadKernelFromDisk(Reason::InvalidElf),
                    Context::LoadingKernel,
                )
                .into()
            })
        }
        Err(_drive_parametrs) => {
            // TODO: try USB
            Err(InternalError::new(
                Bootloader,
                Kind::CantReadKernelFromDisk(Reason::UnsupportedBootMedium),
                Context::LoadingKernel,
            )
            .into())
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {
    let mut args = std::env::args();
    args.next().unwrap();
    let bytes = std::fs::read(args.next().unwrap()).unwrap();
    let elf_file = match elf::File::try_from(bytes.as_slice()) {
        Ok(elf_file) => elf_file,
        Err(err) => {
            println!("{err}");
            return;
        }
    };
    let mut s = String::new();
    writeln!(s, "{}", elf_file.header()).unwrap();
    print!("{s}");

    let string_table = elf_file
        .get_section_by_index(elf_file.header().string_table_index().into())
        .unwrap()
        .unwrap()
        .downcast_to_string_table()
        .unwrap();

    println!("--------");
    println!("SECTIONS");
    println!("--------");
    for section in elf_file.sections() {
        use core::fmt::Write as _;

        let section = section.unwrap();

        let mut s = String::new();
        let section_name = string_table
            .get_string(section.name_index() as usize)
            .unwrap()
            .unwrap();
        s.write_fmt(format_args!("Section name: {section_name}\n"))
            .unwrap();
        section.write_to(&mut s).unwrap();
        println!("--------");
        print!("{s}");
        println!("--------");
    }

    println!("--------");
    println!("SEGMENTS");
    println!("--------");
    for header in elf_file.program_headers() {
        let header = header.unwrap();

        let mut s = String::new();
        write!(&mut s, "{header}").unwrap();
        println!("--------");
        print!("{s}");
        println!("--------");
    }
}
