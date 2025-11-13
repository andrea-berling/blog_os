#[macro_export]
macro_rules! make_bitmap {
    (new_type: $flags_type:ident, underlying_flag_type: $flag_type:ty, repr: $flag_unsigned_type:ty$(, bit_skipper: $skip_bit:expr)?) => {
        make_bitmap!(new_type: $flags_type, underlying_flag_type: $flag_type, repr: $flag_unsigned_type, nodisplay);

        impl ::core::fmt::Display for $flags_type {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                let mut printed_once = false;
                for i in 0..<$flag_unsigned_type>::BITS {
                    if false $(|| $skip_bit(i) )? {
                        continue;
                    }
                    // PANIC: no panics, values have been purposedly chose not to
                    let flag = <$flag_type>::try_from(1 << i).unwrap();
                    if self.is_set(flag) {
                        if printed_once {
                            write!(f, "|")?;
                        }
                        write!(f, "{}", flag)?;
                        printed_once = true;
                    }
                }
                Ok(())
            }
        }
    };
    (new_type: $flags_type:ident, underlying_flag_type: $flag_type:ty, repr: $flag_unsigned_type:ty, nodisplay) => {
        #[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
        pub struct $flags_type($flag_unsigned_type);

        #[allow(unused)]
        impl $flags_type {
            pub const fn empty() -> Self {
                $flags_type(0)
            }

            fn is_set(&self, flag: $flag_type) -> bool {
                self.0 & (flag as $flag_unsigned_type) != 0
            }

            pub fn set_flag(&mut self, flag: $flag_type) {
                self.0 |= flag as $flag_unsigned_type;
            }

            fn clear_flag(&mut self, flag: $flag_type) {
                self.0 &= !(flag as $flag_unsigned_type);
            }
        }

        impl From<$flag_unsigned_type> for $flags_type {
            fn from(value: $flag_unsigned_type) -> Self {
                Self(value)
            }
        }

        impl From<$flags_type> for $flag_unsigned_type {
            fn from(value: $flags_type) -> Self {
                value.0
            }
        }

        impl From<$flag_type> for $flags_type {
            fn from(value: $flag_type) -> Self {
                let mut result = $flags_type(0);
                result.set_flag(value);
                result
            }
        }

        impl core::ops::BitOr for $flag_type {
            type Output = $flags_type;

            fn bitor(self, rhs: Self) -> Self::Output {
                let mut flags = $flags_type(0);
                flags.set_flag(self);
                flags.set_flag(rhs);
                flags
            }
        }

        impl core::ops::BitOr<$flag_type> for $flags_type {
            type Output = $flags_type;

            fn bitor(mut self, rhs: $flag_type) -> Self::Output {
                self.set_flag(rhs);
                self
            }
        }

    };
}
