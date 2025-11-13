#[cfg(target_arch = "x86")]
use core::arch::x86::__cpuid;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::__cpuid;
use core::cmp::min;

use crate::{error::bounded_context, make_bitmap};

#[allow(unused)]
#[repr(u64)]
pub enum PageTableEntryFlag {
    Present = 1 << 0,
    Write = 1 << 1,
    AllowUserModeAccess = 1 << 2,
    PageLevelWriteThrough = 1 << 3,
    PageLevelCacheDisable = 1 << 4,
    Accessed = 1 << 5,
    MapsPage = 1 << 7,
    HLATRestart = 1 << 11,
    ExecuteDisable = 1 << 63,
}

make_bitmap!(new_type: PageTableEntry, underlying_flag_type: PageTableEntryFlag, repr: u64, nodisplay);

#[allow(unused)]
#[repr(u64)]
pub enum PageMappingEntryFlag {
    Dirty = 1 << 6,
    Global = 1 << 8,
    AllowUserModeAccess = 1 << 2,
    PageLevelWriteThrough = 1 << 3,
    PageLevelCacheDisable = 1 << 4,
    Accessed = 1 << 5,
    ExecuteDisable = 1 << 63,
}

make_bitmap!(new_type: PageMappingEntry, underlying_flag_type: PageMappingEntryFlag, repr: u64, nodisplay);

#[allow(unused)]
#[repr(u64)]
pub enum LargePageEntryFlag {
    PageAttributeTable = 1 << 12,
}

make_bitmap!(new_type: LargePageEntry, underlying_flag_type: LargePageEntryFlag, repr: u64, nodisplay);

#[allow(unused)]
#[repr(u32)]
pub enum ExtendedProcessorSignatureAndFeatureBit {
    _1GBPagesAvailable = 1 << 26,
}

make_bitmap!(new_type: ExtendedProcessorSignatureAndFeatures, underlying_flag_type: ExtendedProcessorSignatureAndFeatureBit, repr: u32, nodisplay);

const LINEAR_PHYSICAL_ADDRESS_SIZE: u32 = 0x80000008;
const EXTENDED_PROCESSOR_SIGNATURE_AND_FEATURE_BITS: u32 = 0x80000001;

fn get_max_physical_address_width() -> u8 {
    // SAFETY: The `__cpuid` instruction is safe to call with the given arguments.
    unsafe { __cpuid(LINEAR_PHYSICAL_ADDRESS_SIZE).eax as u8 }
}

fn supports_1gb_pages() -> bool {
    // SAFETY: The `__cpuid` instruction is safe to call with the given arguments.
    let result = unsafe { __cpuid(EXTENDED_PROCESSOR_SIGNATURE_AND_FEATURE_BITS).edx };

    ExtendedProcessorSignatureAndFeatures::from(result)
        .is_set(ExtendedProcessorSignatureAndFeatureBit::_1GBPagesAvailable)
}

macro_rules! impl_deref_to_page_table_entry {
    ($type:ty) => {
        impl core::ops::Deref for $type {
            type Target = PageTableEntry;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl core::ops::DerefMut for $type {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }
    };
}

#[derive(Clone, Copy)]
pub struct PML4Entry(PageTableEntry);

impl_deref_to_page_table_entry!(PML4Entry);

#[repr(align(4096))]
pub struct PML4 {
    pub entries: [PML4Entry; 512],
}

impl Default for PML4 {
    fn default() -> Self {
        Self::new()
    }
}

impl PML4 {
    pub const fn new() -> Self {
        Self {
            entries: [PML4Entry::new(); 512],
        }
    }
}

const ADDRESS_CLEAR_MASK: u64 = !0x7_ffff_ffff_f000;

impl PML4Entry {
    pub const fn new() -> Self {
        Self(PageTableEntry::empty())
    }

    pub fn set_page_directory_pointer_table(&mut self, pdpt: &PageDirectoryPointerTable) {
        self.0.set_flag(PageTableEntryFlag::Present);
        let max_width = get_max_physical_address_width();
        let addr = (pdpt as *const _ as u64) & ((1u64 << max_width) - 1);
        self.0.0 &= ADDRESS_CLEAR_MASK;
        self.0.0 |= addr;
    }
}

impl Default for PML4Entry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
pub struct PageDirectoryPointerTableEntry(PageTableEntry);

impl_deref_to_page_table_entry!(PageDirectoryPointerTableEntry);

pub struct _1GPage(*const u8);

impl TryFrom<*const u8> for _1GPage {
    type Error = crate::error::Reason;

    fn try_from(bytes: *const u8) -> Result<Self, crate::error::Reason> {
        if !supports_1gb_pages() {
            return Err(crate::error::Reason::UnsupportedFeature(
                crate::error::Feature::_1GBPages,
            ));
        }
        Ok(Self(bytes))
    }
}

impl PageDirectoryPointerTableEntry {
    pub const fn new() -> Self {
        Self(PageTableEntry::empty())
    }

    pub fn set_physical_address(&mut self, page: _1GPage) {
        self.0.set_flag(PageTableEntryFlag::Present);
        self.0.set_flag(PageTableEntryFlag::MapsPage);
        let max_physical_width = get_max_physical_address_width();
        let addr = (page.0 as u64) & ((1 << max_physical_width) - 1);
        self.0.0 &= !0x7_ffff_ffff_f000;
        self.0.0 |= addr;
    }

    pub fn set_page_directory(&mut self, page_directory: &'static PageDirectoryTable) {
        self.0.set_flag(PageTableEntryFlag::Present);
        let max_physical_width = get_max_physical_address_width();
        let addr = (page_directory.0.as_ptr() as u64) & ((1 << max_physical_width) - 1);
        self.0.0 &= !0x7_ffff_ffff_f000;
        self.0.0 |= addr;
    }
}

impl Default for PageDirectoryPointerTableEntry {
    fn default() -> Self {
        Self::new()
    }
}

#[repr(align(4096))]
pub struct PageDirectoryPointerTable {
    pub entries: [PageDirectoryPointerTableEntry; 512],
}

impl PageDirectoryPointerTable {
    pub const fn new() -> Self {
        Self {
            entries: [PageDirectoryPointerTableEntry::new(); 512],
        }
    }
}

impl Default for PageDirectoryPointerTable {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
pub struct PageDirectoryEntry(PageTableEntry);

impl_deref_to_page_table_entry!(PageDirectoryEntry);

impl PageDirectoryEntry {
    pub fn set_physical_address(&mut self, page: *const u8) {
        self.0.set_flag(PageTableEntryFlag::Present);
        self.0.set_flag(PageTableEntryFlag::MapsPage);
        let max_physical_width = get_max_physical_address_width();
        let addr = (page as u64) & ((1 << max_physical_width) - 1);
        self.0.0 &= ADDRESS_CLEAR_MASK;
        self.0.0 |= addr;
    }

    pub fn set_page_table(&mut self, page_table: &'static PageTable) {
        self.0.set_flag(PageTableEntryFlag::Present);
        let max_physical_width = min(get_max_physical_address_width(), 39);
        let addr = (page_table.0.as_ptr() as u64) & ((1 << max_physical_width) - 1);
        self.0.0 &= ADDRESS_CLEAR_MASK;
        self.0.0 |= addr;
    }
}

#[repr(align(4096))]
pub struct PageDirectoryTable([PageDirectoryEntry; 512]);

#[repr(align(4096))]
pub struct _4KPage([u8; 0x4096]);

impl PageTableEntry {
    /// Set the address of the pointee
    /// The pointee must be the physical address of a 4K mapped page
    pub fn set_physical_address(&mut self, page: &_4KPage) {
        // TODO: I probably have more places to check alignment for
        let address = page.0.as_ptr() as u64;
        let max_physical_width = get_max_physical_address_width();
        let addr = address & ((1 << max_physical_width) - 1);
        self.0 &= (u64::MAX << max_physical_width).rotate_left(12);
        self.0 |= addr;
    }
}

#[repr(align(4096))]
pub struct PageTable([PageTableEntry; 512]);

#[cfg(test)]
mod tests {
    use crate::paging::{self, PML4Entry};

    #[test]
    fn first_gb_identity_mapped() {
        let mut pdpt = paging::PageDirectoryPointerTable::new();
        pdpt.entries[0].set_physical_address(core::ptr::null::<u8>().try_into().expect("TODO"));
        pdpt.entries[0].set_flag(paging::PageTableEntryFlag::Write);

        assert_eq!([0x83, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,], unsafe {
            core::mem::transmute::<_, [u8; 8]>(pdpt.entries[0])
        });

        let mut pml4_entry = PML4Entry::new();

        pml4_entry.set_page_directory_pointer_table(&pdpt);
        pml4_entry.set_flag(paging::PageTableEntryFlag::Write);

        let pdpt_addr = core::ptr::addr_of!(pdpt) as u64;

        assert_eq!(
            [
                0x3,
                ((pdpt_addr >> 8) as u8),
                ((pdpt_addr >> 16) as u8),
                ((pdpt_addr >> 24) as u8),
                ((pdpt_addr >> 32) as u8),
                ((pdpt_addr >> 40) as u8),
                0x0,
                0x0,
            ],
            unsafe { core::mem::transmute::<_, [u8; 8]>(pml4_entry) }
        );
    }
}
