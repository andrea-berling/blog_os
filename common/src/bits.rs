#[macro_export]
/// Precondition: bits_expr must be a side-effect free (e.g. a simple lvalue)
macro_rules! set_bits {
    (bits_expr: $bits_expr:expr, value: $value:expr, n_bits: $n_bits:literal, starts_at_bit: $starts_at_bit:literal, bits_expr_ty: $bits_expr_ty:ty) => {{
        let __mask: $bits_expr_ty = (((1 as $bits_expr_ty) << $n_bits) - 1) as $bits_expr_ty;
        $bits_expr &= !(__mask << $starts_at_bit);
        $bits_expr |= (($value as $bits_expr_ty) & __mask) << $starts_at_bit;
    }};
}

#[macro_export]
macro_rules! get_bits {
    (bits_expr: $bits_expr:expr, n_bits: $n_bits:literal, starts_at_bit: $starts_at_bit:literal, return_ty: $return_ty:ty) => {{
        let __mask: $return_ty = (((1u128) << $n_bits) - 1) as $return_ty;
        (($bits_expr >> $starts_at_bit) as $return_ty) & __mask
    }};
}

pub use get_bits;
pub use set_bits;
