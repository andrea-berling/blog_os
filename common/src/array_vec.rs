use core::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut, Index, IndexMut},
};

use crate::error::{self, Fault};

pub struct ArrayVec<T, const N: usize> {
    buffer: [MaybeUninit<T>; N],
    len: usize,
}

pub type ArrayVec8<T> = ArrayVec<T, 8>;

impl<T, const N: usize> ArrayVec<T, N> {
    fn as_ptr(&self) -> *const T {
        self.buffer.as_ptr().cast()
    }

    fn as_mut_ptr(&mut self) -> *mut T {
        self.buffer.as_mut_ptr().cast()
    }

    fn as_slice(&self) -> &[T] {
        // SAFETY: items up to self.len have been initialised
        unsafe { core::slice::from_raw_parts(self.as_ptr(), self.len) }
    }

    fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY: items up to self.len have been initialised
        unsafe { core::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len) }
    }

    pub fn new() -> Self {
        Self {
            buffer: [const { MaybeUninit::uninit() }; _],
            len: 0,
        }
    }

    /// Appends `value` to the end of the vector
    ///
    /// # Errors
    /// Fails with [`Fault::FullArrayVec`] if the vector is already at capacity
    pub fn try_push(&mut self, value: T) -> error::Result<()> {
        if self.len == N {
            return Err(Fault::FullArrayVec(N).into());
        }
        self.buffer[self.len].write(value);
        self.len += 1;
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// # Safety
    /// This function assumes internally that it's fine to make a bitwise copy of T, even if T isn't
    /// copy. That generally holds, but not for some edge cases, like interior mutability and
    /// self-referential data types
    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }
        self.len -= 1;
        // SAFETY: all elements up to the old self.len are initialised
        // assume_init_read is fine here despite this creating a "new copy"
        // of the returned value, the old copy lives in an array slot that
        // won't be read anymore before being overwritten with a new value
        Some(unsafe { self.buffer[self.len].assume_init_read() })
    }

    pub fn capacity(&self) -> usize {
        N
    }
}

impl<T, const N: usize> DerefMut for ArrayVec<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<T, const N: usize> Deref for ArrayVec<T, N> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T, const N: usize> IndexMut<usize> for ArrayVec<T, N> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.as_mut()[index]
    }
}

impl<T, const N: usize> Index<usize> for ArrayVec<T, N> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.as_ref()[index]
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a ArrayVec<T, N> {
    type Item = &'a T;

    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a mut ArrayVec<T, N> {
    type Item = &'a mut T;

    type IntoIter = core::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T, const N: usize> Default for ArrayVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Drop for ArrayVec<T, N> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}
