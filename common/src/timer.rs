// https://www.alldatasheet.com/datasheet-pdf/download/66093/INTEL/PIIX3.html
use core::arch::asm;

use crate::make_flags;

const TIMER_0_FREQUENCY_HZ: u32 = 1_193_182;

const TIMER_CONTROL_WORD: u8 = 0x43;
const TIMER_0: u8 = 0x40;

enum Counter {
    _0,
    _1,
    _2,
}

#[allow(unused)]
#[repr(u8)]
enum TimerControlWordFlag {
    BinaryCodedDecimals = 0x1,
    CounterModeBit1 = 0x2,
    CounterModeBit2 = 0x4,
    CounterModeBit3 = 0x8,
    ReadWriteSelectBit1 = 0x10,
    ReadWriteSelectBit2 = 0x20,
    CounterSelectBit1 = 0x40,
    CounterSelectBit2 = 0x80,
}

make_flags!(new_type: TimerControlWordFlags, underlying_flag_type: TimerControlWordFlag, repr: u8, nodisplay);

impl TimerControlWordFlags {
    fn select_counter(mut self, counter: Counter) -> Self {
        use TimerControlWordFlag::*;
        match counter {
            Counter::_0 => {
                self.unset_flag(CounterSelectBit1);
                self.unset_flag(CounterSelectBit2);
            }
            Counter::_1 => {
                self.unset_flag(CounterSelectBit1);
                self.set_flag(CounterSelectBit2);
            }
            Counter::_2 => {
                self.set_flag(CounterSelectBit1);
                self.unset_flag(CounterSelectBit2);
            }
        }
        self
    }

    fn counter_latch(mut self) -> Self {
        use TimerControlWordFlag::*;
        self.unset_flag(ReadWriteSelectBit1);
        self.unset_flag(ReadWriteSelectBit2);
        self
    }

    fn binary_countdown(mut self) -> Self {
        use TimerControlWordFlag::*;
        self.unset_flag(BinaryCodedDecimals);
        self
    }
}

/// Returns the current value of timer zero
fn read_timer_0_counter() -> u16 {
    let timer_control_word = TimerControlWordFlags::empty()
        .select_counter(Counter::_0)
        .counter_latch()
        .binary_countdown();
    let result: u16;
    // SAFETY: registers are correct
    unsafe {
        asm!(
            "out {tcw_reg}, al",
            "in al, {counter0_reg}",
            "in al, {counter0_reg}",
            tcw_reg = const TIMER_CONTROL_WORD,
            in("al") u8::from(timer_control_word),
            counter0_reg = const TIMER_0,
            lateout("ax") result
        );
    }
    result
}

/// Given a number of elapsed ticks, returns roughly how many nanoseconds have passed
fn nanoseconds_elapsed_timer_0(ticks: u32) -> u64 {
    ((ticks as f64 / TIMER_0_FREQUENCY_HZ as f64) * 1e9) as u64
}

#[derive(Debug)]
pub struct LowPrecisionTimer {
    original_ticks: u64,
    ticks: u64,
    started: bool,
    last_counter_value: u16,
}

impl LowPrecisionTimer {
    pub fn new(timeout_ns: u64) -> Self {
        // TODO: bound checks probably
        let ticks = (timeout_ns as f64 * TIMER_0_FREQUENCY_HZ as f64 / 1e9) as u64;
        Self {
            original_ticks: ticks,
            ticks,
            started: false,
            last_counter_value: 0,
        }
    }

    pub fn timeout(&self) -> bool {
        self.ticks == 0
    }

    pub fn update(&mut self) {
        let counter = read_timer_0_counter();

        if !self.started {
            self.started = true;
            self.last_counter_value = counter;
            return;
        }

        self.ticks = self
            .ticks
            .saturating_sub(self.last_counter_value.wrapping_sub(counter) as u64);
        self.last_counter_value = counter;
    }

    pub fn reset(&mut self) {
        self.ticks = self.original_ticks;
        self.started = false;
    }
}
