use core::{fmt::Display, marker::PhantomData};

/// Volatile access to a `T` located at a raw address (typically an MMIO register).
///
/// # Safety model
/// The unsafety lives entirely at construction ([`Self::from_raw`]): constructing a
/// `VolatilePtr` certifies that the pointee satisfies the volatile-access conditions
/// (mapped, uncached, valid `T` bit patterns, no concurrent access) for the whole
/// lifetime of the value. Every accessor is then safe and simply inherits those
/// guarantees.
///
/// The `From<&mut T>` and `From<&VolatileValue<T>>` conversions are safe side-doors: a
/// live reference guarantees the pointee is a valid, aligned, initialized `T` at a
/// mapped address, but NOT the cacheability or concurrency conditions; those remain the
/// caller's obligation, inherited from whoever placed the value in memory
#[derive(Debug)]
pub struct VolatilePtr<T> {
    address: *mut u8,
    _phantom: PhantomData<T>,
}

impl<T> VolatilePtr<T> {
    /// Creates a `VolatilePtr` to the `T` at `address`
    ///
    /// # Safety
    /// By constructing a `VolatilePtr`, the caller certifies, for the whole lifetime of
    /// the value, that:
    ///   - `address` points to a mapped (or MMIO) location that is valid, aligned and,
    ///     for reads, initialized for `T`
    ///   - every bit pattern the location can ever hold (including ones the hardware may
    ///     write) is a valid `T`
    ///   - the location is not mapped cacheable: volatile access only constrains the
    ///     compiler, not the CPU cache
    ///   - no other agent (another core, a DMA-capable device) accesses the location
    ///     concurrently with the accesses performed through the `VolatilePtr`
    ///
    /// All accessors are then safe: they inherit these guarantees
    pub const unsafe fn from_raw(address: *mut T) -> Self {
        Self {
            address: address.cast(),
            _phantom: PhantomData,
        }
    }

    /// Volatile write of a whole `T` value at the register
    pub fn set(&mut self, value: T) {
        // SAFETY: construction invariant of `VolatilePtr` (see `from_raw`)
        unsafe {
            self.address.cast::<T>().write_volatile(value);
        }
    }

    /// Volatile read-modify-write of the whole `T` value at the register
    ///
    /// # Note
    /// This is NOT atomic with respect to hardware updates of the same register: on
    /// registers with write-1-to-clear (RW1C) bits (e.g. EHCI PORTSC change bits), the
    /// write-back re-writes the value as read, so any RW1C bit that was set at read time
    /// gets cleared even if `f` didn't intend to. Only use on registers where that is
    /// acceptable
    pub fn update(&mut self, f: impl FnOnce(&mut T)) {
        let current_ptr: *mut T = self.address.cast();
        // SAFETY: construction invariant of `VolatilePtr` (see `from_raw`)
        let mut current_val = unsafe { current_ptr.read_volatile() };
        f(&mut current_val);
        // SAFETY: construction invariant of `VolatilePtr` (see `from_raw`)
        unsafe {
            current_ptr.write_volatile(current_val);
        }
    }

    /// Volatile read of the whole `T` value at the register, passed to `f` by reference
    pub fn read_with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let current_ptr: *const T = self.address.cast();
        // SAFETY: construction invariant of `VolatilePtr` (see `from_raw`)
        let current_val = unsafe { current_ptr.read_volatile() };
        f(&current_val)
    }
}

impl<T: Copy> VolatilePtr<T> {
    pub fn read(&self) -> T {
        self.read_with(|x| *x)
    }
}

impl<T: Clone> VolatilePtr<T> {
    pub fn clone_read(&self) -> T {
        self.read_with(T::clone)
    }
}

#[repr(transparent)]
pub struct VolatileValue<T>(T);

// NOTE on safety: `VolatileValue` discharges the address-validity part of the
// [`VolatilePtr`] construction contract by construction (it is a live,
// `repr(transparent)` wrapper around a `T`, so the address is valid, aligned and
// `T`-typed). The remaining conditions (non-cacheable mapping, no concurrent hardware
// or software access) are obligations of whoever placed the value in memory: in this
// codebase, `VolatileValue`s live inside MMIO-mapped, zero-initialized, statically
// allocated EHCI data structures accessed by a single thread

impl<T: Copy> VolatileValue<T> {
    pub fn read(&self) -> T {
        self.into_ptr().read()
    }
}

impl<T> VolatileValue<T> {
    fn into_ptr(&self) -> VolatilePtr<T> {
        VolatilePtr::<T>::from(self)
    }

    pub fn set(&mut self, value: T) {
        self.into_ptr().set(value)
    }

    pub fn update(&mut self, f: impl FnOnce(&mut T)) {
        self.into_ptr().update(f)
    }

    pub fn read_with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.into_ptr().read_with(f)
    }
}

pub trait Maskable {
    type Mask;

    fn mask(self, mask: Self::Mask) -> Self;
    fn blend(self, other: Self, mask: Self::Mask) -> Self;
}

impl<T> From<&mut T> for VolatilePtr<T> {
    /// NOTE: this safe conversion only guarantees what a live `&mut T` guarantees: the
    /// pointee is a valid, aligned, initialized `T` at a mapped address. The remaining
    /// volatile conditions (non-cacheable mapping, no concurrent access) are inherited
    /// from whoever placed the value in memory
    fn from(address: &mut T) -> Self {
        Self {
            address: (&raw mut *address).cast(),
            _phantom: PhantomData,
        }
    }
}

impl<T> From<&mut VolatileValue<T>> for VolatilePtr<T> {
    fn from(value: &mut VolatileValue<T>) -> Self {
        VolatilePtr {
            // SAFETY: `value` is a live reference, so it points to a valid, initialized,
            // aligned `T`; `VolatileValue` is `repr(transparent)` over `T`, so the cast
            // preserves both address and type
            address: (&raw mut *value).cast(),
            _phantom: PhantomData,
        }
    }
}

impl<T> From<&VolatileValue<T>> for VolatilePtr<T> {
    fn from(value: &VolatileValue<T>) -> Self {
        VolatilePtr {
            // SAFETY: same as the `&mut` conversion above: live reference to a valid,
            // initialized, aligned `T`, and `repr(transparent)` keeps the cast faithful.
            // The `cast_mut` only widens for later volatile reads, never written through
            // unless the caller goes through `&mut VolatileValue` (which needs a live
            // mutable reference)
            address: (&raw const *value).cast_mut().cast(),
            _phantom: PhantomData,
        }
    }
}

impl<T: Display> Display for VolatileValue<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.read_with(|value| write!(f, "{value}",))
    }
}

impl<T: Maskable + Clone> VolatilePtr<T> {
    /// Volatile read-modify-write of the whole `T` value at the register, restricted to the
    /// non-masked out parts of T, regardless of what f does to the passed value
    pub fn update_masked(&mut self, mask: T::Mask, f: impl FnOnce(&mut T)) {
        let current_ptr: *mut T = self.address.cast();
        // SAFETY: construction invariant of `VolatilePtr` (see `from_raw`)
        let mut current_val = unsafe { current_ptr.read_volatile() };
        let prev_val = current_val.clone();
        f(&mut current_val);
        // SAFETY: construction invariant of `VolatilePtr` (see `from_raw`)
        unsafe {
            current_ptr.write_volatile(prev_val.blend(current_val, mask));
        }
    }
}
