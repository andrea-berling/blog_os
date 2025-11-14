use crate::make_bitmap;

#[allow(unused)]
#[repr(u8)]
pub enum SelectorBit {
    UseLocalDescriptorTable = 1 << 2,
}

make_bitmap!(new_type: Selector, underlying_flag_type: SelectorBit, repr: u8, nodisplay);

impl Selector {
    pub fn with_index(index: u8) -> Self {
        let mut result = Self::empty();
        result.bits &= 0x7;
        result.bits |= index << 3;
        result
    }
}

#[derive(Default, Clone, Copy, Debug)]
#[repr(C, packed)]
pub struct TaskStateSegment {
    previous_task_link: u16,
    reserved1: u16,
    esp0: u32,
    ss0: u16,
    reserved2: u16,
    esp1: u32,
    ss1: u16,
    reserved3: u16,
    esp2: u32,
    ss2: u16,
    reserved4: u16,
    page_directory_base_register: u32,
    eip: u32,
    eflags: u32,
    eax: u32,
    ecx: u32,
    edx: u32,
    ebx: u32,
    esp: u32,
    ebp: u32,
    esi: u32,
    edi: u32,
    es: u16,
    reserved5: u16,
    cs: u16,
    reserved6: u16,
    ss: u16,
    reserved7: u16,
    ds: u16,
    reserved8: u16,
    fs: u16,
    reserved9: u16,
    gs: u16,
    reserved10: u16,
    ldt_segment_selector: u16,
    reserved11: u16,
    debug: bool,
    reserved12: u8,
    io_permission_map_base_address: u16,
    ssp: u32,
}

#[repr(align(16))]
pub struct Stack<const SIZE: usize>([u8; SIZE]);

impl<const SIZE: usize> Stack<SIZE> {
    pub const fn new(backing_buffer: [u8; SIZE]) -> Self {
        Self(backing_buffer)
    }
}

impl TaskStateSegment {
    pub const fn blank() -> Self {
        Self {
            previous_task_link: 0,
            reserved1: 0,
            esp0: 0,
            ss0: 0,
            reserved2: 0,
            esp1: 0,
            ss1: 0,
            reserved3: 0,
            esp2: 0,
            ss2: 0,
            reserved4: 0,
            page_directory_base_register: 0,
            eip: 0,
            eflags: 0,
            eax: 0,
            ecx: 0,
            edx: 0,
            ebx: 0,
            esp: 0,
            ebp: 0,
            esi: 0,
            edi: 0,
            es: 0,
            reserved5: 0,
            cs: 0,
            reserved6: 0,
            ss: 0,
            reserved7: 0,
            ds: 0,
            reserved8: 0,
            fs: 0,
            reserved9: 0,
            gs: 0,
            reserved10: 0,
            ldt_segment_selector: 0,
            reserved11: 0,
            debug: false,
            reserved12: 0,
            io_permission_map_base_address: 0,
            ssp: 0,
        }
    }
    pub fn with_ss0_stack<const N: usize>(segment: u16, stack: &Stack<N>) -> Self {
        Self {
            // NOTE: better explode here than using `as` and silently proceeding
            ss0: segment,
            // TODO: what if start + len is not aligned?
            esp0: (stack.0.as_ptr() as usize + stack.0.len()) as u32,
            io_permission_map_base_address: size_of::<TaskStateSegment>() as u16,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::gdt;
    use crate::tss;
    use crate::tss::Selector;

    #[test]
    fn _32bit_tss() {
        let stack = tss::Stack::new([0; 1024]);
        let stack_addr = core::ptr::addr_of!(stack) as u32;
        let tss = tss::TaskStateSegment::with_ss0_stack(0x10, &stack);

        // TSS tests
        assert_eq!(stack_addr, { tss.esp0 } - stack.0.len() as u32);
        assert_eq!(0x10, { tss.ss0 });
        assert_eq!(
            size_of::<tss::TaskStateSegment>(),
            tss.io_permission_map_base_address as usize
        );
        let esp0 = tss.esp0;
        assert_eq!(
            [
                0,
                0,
                0,
                0,
                esp0 as u8,
                (esp0 >> 8) as u8,
                (esp0 >> 16) as u8,
                (esp0 >> 24) as u8,
                0x10,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                size_of::<tss::TaskStateSegment>() as u8,
                0,
                0,
                0,
                0,
                0
            ],
            unsafe { core::mem::transmute::<tss::TaskStateSegment, [u8; 108]>(tss) }
        );

        // Descriptors tests
        let tss_descriptor = gdt::SegmentDescriptor::new_tss(&tss);
        let tss_addr = core::ptr::addr_of!(tss) as u32;

        assert_eq!(tss_addr, tss_descriptor.get_base());
        assert_eq!(
            size_of::<tss::TaskStateSegment>() as u32 - 4 - 1,
            tss_descriptor.get_limit()
        );
        assert!(tss_descriptor.is_present());
        assert!(!tss_descriptor.has_4k_granularity());
        assert!(tss_descriptor.is_tss());
        assert_eq!(
            [
                0x67,
                0,
                tss_addr as u8,
                (tss_addr >> 8) as u8,
                (tss_addr >> 16) as u8,
                0x89,
                0,
                (tss_addr >> 24) as u8
            ],
            unsafe { core::mem::transmute::<gdt::SegmentDescriptor, [u8; 8]>(tss_descriptor) }
        );
    }

    #[test]
    fn selector() {
        let selector = Selector::with_index(5);
        assert_eq!(5 << 3, u8::from(selector));
        assert!(!selector.is_set(tss::SelectorBit::UseLocalDescriptorTable))
    }
}
