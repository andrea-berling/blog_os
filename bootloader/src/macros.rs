#[macro_export]
macro_rules! make_flags {
    (new_type: $type_name:ident, underlying_flag_type: $flag_type:ty, repr: $flag_unsigned_type:ty$(, bit_skipper: $skip_bit:expr)?) => {
        #[derive(Debug)]
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
        }
    };
}
