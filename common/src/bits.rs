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

use core::marker::PhantomData;

use crate::error::{self, Fault};

pub use get_bits;
pub use set_bits;

macro_rules! small_bit_set_backing_type {
    () => {
        u16
    };
}

#[derive(Clone)]
pub struct SmallBitSet<T: Into<usize> + From<usize>> {
    bits: u16,
    max_size: usize,
    _phantom: PhantomData<T>,
}

pub struct SmallBitSetPoppingIterator<'a, T: Into<usize> + From<usize>> {
    bitset: &'a mut SmallBitSet<T>,
    n_items: usize,
}

impl<T: Into<usize> + From<usize>> SmallBitSet<T> {
    /// Creates a bit set that can hold indexes below `max_size`
    ///
    /// # Safety
    /// `max_size` must not exceed the backing type's bit width (16): `fill` shifts
    /// `1 << max_size`, which would overflow. Prefer [`Self::try_new`], which returns a
    /// fault instead
    ///
    /// # Panics
    /// Panics if `max_size` exceeds 16 (as a compile-time error in const context). This
    /// assert is a guard rail, not an API contract: callers must not rely on it
    pub const unsafe fn new(max_size: usize) -> Self {
        assert!(max_size <= <small_bit_set_backing_type!()>::BITS as usize);
        Self {
            bits: 0,
            max_size,
            _phantom: PhantomData,
        }
    }

    pub fn try_new(max_size: usize) -> error::Result<Self> {
        let capacity = <small_bit_set_backing_type!()>::BITS as usize;
        if max_size > capacity {
            return Err(Fault::BitSetSizeTooBig {
                desired_size: max_size,
                capacity,
            }
            .into());
        }
        Ok(Self {
            bits: 0,
            max_size,
            _phantom: PhantomData,
        })
    }

    pub const fn fill(&mut self) {
        self.bits = if self.max_size == 0 {
            0
        } else {
            <small_bit_set_backing_type!()>::MAX
                >> (<small_bit_set_backing_type!()>::BITS as usize - self.max_size)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    pub fn size(&self) -> usize {
        self.bits.count_ones() as usize
    }

    /// # Errors
    /// Fails with [`Fault::OutOfBoundsBitSetIndex`] if `i` is not below the set's max size
    pub fn insert(&mut self, i: T) -> error::Result<()> {
        let i: usize = i.into();
        if i >= self.max_size {
            return Err(Fault::OutOfBoundsBitSetIndex {
                index: i,
                max_size: self.max_size,
            }
            .into());
        }
        self.bits |= 1 << i;
        Ok(())
    }

    /// Returns `false` if `i` is not below the set's max size
    pub fn contains(&self, i: T) -> bool {
        let i: usize = i.into();
        if i >= self.max_size {
            return false;
        }
        self.bits & (1 << i) != 0
    }

    /// # Errors
    /// Fails with [`Fault::OutOfBoundsBitSetIndex`] if `i` is not below the set's max size
    pub fn remove(&mut self, i: T) -> error::Result<()> {
        let i: usize = i.into();
        if i >= self.max_size {
            return Err(Fault::OutOfBoundsBitSetIndex {
                index: i,
                max_size: self.max_size,
            }
            .into());
        }
        self.remove_unchecked(i);
        Ok(())
    }

    /// Removes the bit at `i` without any bounds check
    fn remove_unchecked(&mut self, i: usize) {
        self.bits &= !(1 << i);
    }

    pub fn highest_set(&self) -> Option<usize> {
        let highest_set: usize =
            (<small_bit_set_backing_type!()>::BITS - self.bits.leading_zeros()) as usize;
        if highest_set > 0 {
            // highest_set is "positional", we want it in "shift from the left" format
            // -1 is all we need
            Some(highest_set - 1)
        } else {
            None
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        self.highest_set().map(|elem| {
            self.remove_unchecked(elem);
            elem.into()
        })
    }

    /// Returns a popping iterator, which removes items from the original bitset while iterating
    ///
    /// # Avoiding leaks
    ///
    /// A piece of code like the following might lead to leaks, if try_push failed before the
    /// iterator has been completely consumed:
    ///
    /// for item in bitset.take(n) {
    ///     result.try_push(queue_head)?;
    /// }
    ///
    /// Checking beforehand that all the error containing paths are non-reachable and exiting early
    /// is the recommended pattern to avoid this:
    ///
    /// if n > bitset.len() { return Err(Error::NotEnoughItems)}
    /// if result.capacity < n { return Err(Error::TooManyItemsRequested)}
    pub fn take(&mut self, n_items: usize) -> SmallBitSetPoppingIterator<'_, T> {
        SmallBitSetPoppingIterator {
            bitset: self,
            n_items,
        }
    }
}

impl<T: Into<usize> + From<usize>> Iterator for SmallBitSetPoppingIterator<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.n_items > 0 {
            self.n_items -= 1;
            self.bitset.pop()
        } else {
            None
        }
    }
}
