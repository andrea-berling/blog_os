use core::arch::asm;

pub struct Port {
    port_number: u16,
}

impl Port {
    pub fn new(port_number: u16) -> Self {
        Self { port_number }
    }

    pub fn writeb(&self, byte: u8) {
        // SAFETY: It is assumed that the user initialised this port with a valid port number
        unsafe {
            asm! {
                "out dx, al", in("dx") self.port_number, in("al") byte,
                options(nomem, nostack, preserves_flags)
            }
        }
    }

    pub fn writew(&self, word: u16) {
        // SAFETY: It is assumed that the user initialised this port with a valid port number
        unsafe {
            asm! {
                "out dx, ax", in("dx") self.port_number, in("ax") word,
                options(nomem, nostack, preserves_flags)
            }
        }
    }

    pub fn writed(&self, dword: u32) {
        // SAFETY: It is assumed that the user initialised this port with a valid port number
        unsafe {
            asm! {
                "out dx, eax", in("dx") self.port_number, in("eax") dword,
                options(nomem, nostack, preserves_flags)
            }
        }
    }

    pub fn readb(&self) -> u8 {
        let result: u8;
        // SAFETY: It is assumed that the user initialised this port with a valid port number
        unsafe {
            asm! {
                "in al, dx", in("dx") self.port_number, out("al") result,
                options(nomem, nostack, preserves_flags)
            }
        }
        result
    }

    pub fn readd(&self) -> u32 {
        let result: u32;
        // SAFETY: It is assumed that the user initialised this port with a valid port number
        unsafe {
            asm! {
                "in eax, dx", in("dx") self.port_number, out("eax") result,
                options(nomem, nostack, preserves_flags)
            }
        }
        result
    }

    pub fn rep_insw(&self, output_buffer: &mut [u8], n_words: u16) -> Result<(), u16> {
        if output_buffer.len() / size_of::<u16>() != n_words as usize {
            return Err(n_words);
        }
        // SAFETY: It is assumed that the user initialised this port with a valid port number
        unsafe {
            asm!("rep insw",
                in("dx") self.port_number,
                in("edi") output_buffer.as_mut_ptr(),
                // u16 is the size of word
                in("cx") n_words,
                options(nostack, preserves_flags)
            );
        }
        Ok(())
    }
}
