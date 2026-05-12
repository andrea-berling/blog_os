use num_traits::AsPrimitive;

pub struct Volatile {
    address: *mut u8,
}

impl Volatile {
    pub fn new<T: AsPrimitive<usize>>(address: T) -> Self {
        Self {
            address: address.as_() as *mut u8,
        }
    }

    pub fn writeb(&self, byte: u8) {
        // SAFETY: It is assumed that the user initialised this port with a valid address
        unsafe {
            core::ptr::write_volatile(self.address, byte);
        }
    }

    pub fn writew(&self, word: u16) {
        // SAFETY: It is assumed that the user initialised this port with a valid address
        unsafe {
            core::ptr::write_volatile(self.address as *mut u16, word);
        }
    }

    pub fn writed(&self, dword: u32) {
        // SAFETY: It is assumed that the user initialised this port with a valid address
        unsafe {
            core::ptr::write_volatile(self.address as *mut u32, dword);
        }
    }

    pub fn readb(&self) -> u8 {
        // SAFETY: It is assumed that the user initialised this port with a valid address
        unsafe { core::ptr::read_volatile(self.address) }
    }

    pub fn readw(&self) -> u16 {
        // SAFETY: It is assumed that the user initialised this port with a valid address
        unsafe { core::ptr::read_volatile(self.address as *const u16) }
    }

    pub fn readd(&self) -> u32 {
        // SAFETY: It is assumed that the user initialised this port with a valid address
        unsafe { core::ptr::read_volatile(self.address as *const u32) }
    }

    pub fn readq(&self) -> u64 {
        // SAFETY: It is assumed that the user initialised this port with a valid address
        unsafe { core::ptr::read_volatile(self.address as *const u64) }
    }
}
