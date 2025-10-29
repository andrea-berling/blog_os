#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(unused)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// So that ColorCode has the exact same data layout as u8
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    const fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

#[repr(transparent)]
struct Buffer {
    chars: [[ScreenChar; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

// Buffer has the same layout as Buffer.chars, and each element of Buffer.chars has the same layout
// as u16
const VGA_BUF: *mut Buffer = 0xb8000 as *mut Buffer;

pub struct Writer {
    column_position: usize,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
}

impl Writer {
    pub const fn new() -> Self {
        // SAFETY: VGA_BUF is the same throughout execution, and ScreenChar has alignment 1
        let buf_ref: Option<&'static mut Buffer> = unsafe { VGA_BUF.as_mut() };
        Self {
            column_position: 0,
            color_code: ColorCode::new(Color::White, Color::Black),
            // SAFETY: VGA_BUF is not null as defined above
            buffer: unsafe { buf_ref.unwrap_unchecked() },
        }
    }

    fn write_screen_char(&mut self, row: usize, col: usize, screen_char: ScreenChar) {
        if row >= BUFFER_HEIGHT || col >= BUFFER_WIDTH {
            return;
        }
        // SAFETY: row and col are within bounds
        unsafe {
            core::ptr::write_volatile(
                core::ptr::from_mut(&mut self.buffer.chars[row][col]),
                screen_char,
            );
        }
    }

    fn read_screen_char(&self, row: usize, col: usize) -> Option<ScreenChar> {
        if row >= BUFFER_HEIGHT || col >= BUFFER_WIDTH {
            return None;
        }
        // SAFETY: row and col are within bounds
        unsafe {
            Some(core::ptr::read_volatile(core::ptr::from_ref(
                &self.buffer.chars[row][col],
            )))
        }
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;

                let color_code = self.color_code;
                self.write_screen_char(
                    row,
                    col,
                    ScreenChar {
                        ascii_character: byte,
                        color_code,
                    },
                );
                self.column_position += 1;
            }
        }
    }

    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let Some(character) = self.read_screen_char(row, col) else {
                    return;
                };
                self.write_screen_char(row - 1, col, character);
            }
        }

        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        for col in 0..BUFFER_WIDTH {
            self.write_screen_char(
                row,
                col,
                ScreenChar {
                    ascii_character: b' ',
                    color_code: self.color_code,
                },
            );
        }
    }

    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                // printable ASCII byte or newline
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                // not part of printable ASCII range
                _ => self.write_byte(0xfe),
            }
        }
    }
}

impl core::fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_string(s);
        Ok(())
    }
}
