#[macro_export]
macro_rules! make_flags {
    (new_type: $type_name:ident, underlying_flag_type: $flag_type:ty, repr: $flag_unsigned_type:ty$(, bit_skipper: $skip_bit:expr)?) => {
        #[derive(Debug, Default, PartialEq, Eq)]
        pub struct $type_name($flag_unsigned_type);

        impl Display for $type_name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                for i in 0..<$flag_unsigned_type>::BITS {
                    if false $(|| $skip_bit(i) )? {
                        continue;
                    }
                    // PANIC: no panics, values have been purposedly chose not to
                    let flag = <$flag_type>::try_from(1 << i).unwrap();
                    if self.is_set(flag) {
                        if i > 1 {
                            write!(f, "|")?;
                        }
                        write!(f, "{}", flag)?;
                    }
                }
                Ok(())
            }
        }

        impl $type_name {
            pub fn is_set(&self, flag: $flag_type) -> bool {
                self.0 & (flag as $flag_unsigned_type) != 0
            }

            fn set_flag(&mut self, flag: $flag_type) {
                self.0 |= flag as $flag_unsigned_type;
            }
        }


        impl From<$flag_type> for $type_name {
            fn from(value: $flag_type) -> Self {
                let mut result = $type_name(0);
                result.set_flag(value);
                result
            }
        }

        impl core::ops::BitOr for $flag_type {
            type Output = $type_name;

            fn bitor(self, rhs: Self) -> Self::Output {
                let mut flags = $type_name(0);
                flags.set_flag(self);
                flags.set_flag(rhs);
                flags
            }
        }

        impl core::ops::BitOr<$flag_type> for $type_name {
            type Output = $type_name;

            fn bitor(mut self, rhs: $flag_type) -> Self::Output {
                self.set_flag(rhs);
                self
            }
        }
    };
}
