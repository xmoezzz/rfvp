#![macro_use]

/// Aligns a value up based on the provided alignment which should be a power of two.
///
/// # Arguments
///
/// * `val`: Expression that should resolve to an unsigned primitive integer type
/// * `alignement`: Alignement value, that should resolve to the same type as `val`
///
/// # Examples
///
/// ```
/// let new_buffer_with_aligned_size = nx::mem::alloc::Buffer::new(size_of::<u128>(), max_object_count);
/// ```
#[macro_export]
macro_rules! align_up {
    ($val:expr, $alignment:expr ) => {{
        debug_assert!(
            ($alignment).is_power_of_two(),
            "Alignment value must be a power of two"
        );
        let align_mask = ($alignment) - 1;
        (($val) + align_mask) & !align_mask
    }};
}

/// Checks if the provided value is aligned to the provided alignment
///
/// # Arguments
///
/// * `val`: Expression that should resolve to an unsigned primitive integer type
/// * `alignment`: Alignement value, that should resolve to the same type as `val`
///
/// # Examples
///
/// ```
/// debug_assert!(is_aligned!(buffer.ptr, align_of::<T>()));
/// ```
#[macro_export]
macro_rules! is_aligned {
    ($val:expr, $alignment:expr ) => {{
        debug_assert!(
            ($alignment).is_power_of_two(),
            "Alignment value must be a power of two"
        );
        let align_mask = ($alignment) - 1;
        ($val) & align_mask == 0
    }};
}

/// Gets a value corresponding to the given bit
///
/// # Arguments
///
/// * `val`: Bit index
///
/// # Examples
///
/// ```
/// assert_eq!(bit!(0), 0b1);
/// assert_eq!(bit!(1), 0b10);
/// assert_eq!(bit!(5), 0b100000);
/// ```
#[macro_export]
macro_rules! bit {
    ($val:expr) => {
        (1 << $val)
    };
}

/// Defines a type meant to serve as a bitflag set with enum-like API
///
/// # Examples
///
/// ```
/// define_bit_set! {
///    Test (u32) {
///        A = bit!(1),
///        B = bit!(2)
///    }
/// }
/// ```
#[macro_export]
macro_rules! define_bit_set {
    (
        $(#[$a_meta:meta])*
        $name:ident ($base:ty) {
            $(
                $(#[$b_meta:meta])*
                $entry_name:ident = $entry_value:expr
            ),*
        }
    ) => {
        $(#[$a_meta])*
        #[derive($crate::ipc::sf::Request, $crate::ipc::sf::Response, Copy, Clone, PartialEq, Eq, Debug, Default)]
        #[repr(C)]
        pub struct $name($base);

        #[allow(non_snake_case)]
        impl $name {
            #[doc = concat!("Creates a `", stringify!($name), "` from the underlying base type `", stringify!($base), "`")]
            pub const fn from(val: $base) -> Self {
                Self(val)
            }

            #[doc = concat!("Checks if the provided `", stringify!($name), "` has all of the set bits in `other` are set in `self`")]
            pub const fn contains(self, other: Self) -> bool {
                (self.0 & other.0) == other.0
            }

            #[doc = concat!("Checks if the provided ", stringify!($name), " has any common bits with `other`")]
            pub const fn intersects(self, other: Self) -> bool {
                (self.0 & other.0) != 0
            }

            /// Returns the value as the underlying type
            pub const fn get(self) -> $base {
                self.0
            }

            $(
                #[doc = concat!("Returns a `", stringify!($name), "` where only the bit for `", stringify!($entry_name), "` is set")]
                $(#[$b_meta])*
                pub const fn $entry_name() -> Self {
                    Self($entry_value)
                }
            )*
        }

        impl core::ops::BitOr for $name {
            type Output = Self;

            #[inline]
            fn bitor(self, other: Self) -> Self {
                Self(self.0 | other.0)
            }
        }

        impl core::ops::BitAnd for $name {
            type Output = Self;

            #[inline]
            fn bitand(self, other: Self) -> Self {
                Self(self.0 & other.0)
            }
        }

        impl core::ops::BitOrAssign for $name {
            #[inline]
            fn bitor_assign(&mut self, other: Self) {
                self.0 |= other.0
            }
        }

        impl core::ops::BitAndAssign for $name {
            #[inline]
            fn bitand_assign(&mut self, other: Self) {
                self.0 &= other.0
            }
        }

        impl core::ops::Not for $name {
            type Output = Self;

            #[inline]
            fn not(self) -> Self {
                Self(!self.0)
            }
        }
    };
}

/// Constructs a `define_bit_set` type value from various flags
///
/// # Examples
///
/// ```
/// define_bit_set! {
///    Test (u32) {
///        A = bit!(1),
///        B = bit!(2)
///    }
/// }
///
/// // The equivalent to what would be "A | B"
/// let test_ab = bit_group! { Test [A, B] };
/// ```
#[macro_export]
macro_rules! bit_group {
    ($base:ty [ $( $val:ident ),* ]) => {
        <$base>::from( $( <$base>::$val().get() )|* )
    };
}

/// Writes bits into a given value
///
/// # Arguments
///
/// * `start`: The start bit index (inclusive)
/// * `end`: The end bit index (inclusive)
/// * `value`: The value to write into
/// * `data`: The value to set
///
/// # Examples
///
/// ```
/// let value = 0u8;
/// write_bits!(0, 3, value, 0b0110);
/// write_bits!(4, 7, value, 0b1001);
/// assert_eq!(value, 0b10010110);
/// ```
#[macro_export]
macro_rules! write_bits {
    ($start:expr, $end:expr, $value:expr, $data:expr) => {
        $value = ($value & (!(((1 << ($end - $start + 1)) - 1) << $start))) | ($data << $start);
    };
}

/// Reads bits from a given value
///
/// # Arguments
///
/// * `start`: The start bit index (inclusive)
/// * `end`: The end bit index (inclusive)
/// * `value`: The value
///
/// # Examples
///
/// ```
/// let value = 0b11110000u8;
/// assert_eq!(read_bits!(value, 0, 3), 0b0000);
/// assert_eq!(read_bits!(value, 4, 7), 0b1111);
/// ```
#[macro_export]
macro_rules! read_bits {
    ($start:expr, $end:expr, $value:expr) => {
        ($value & (((1 << ($end - $start + 1)) - 1) << $start)) >> $start
    };
}

/// Gets the current function name
///
/// # Examples
///
/// ```
/// fn test() {
///     assert_eq!(cur_fn_name!(), "test");
/// }
/// ```
#[macro_export]
macro_rules! cur_fn_name {
    () => {{
        fn dummy_fn() {}
        const DUMMY_FN_EXTRA_SIZE: usize = "::dummy_fn".len();

        fn type_name_of<T>(_: T) -> &'static str {
            core::any::type_name::<T>()
        }

        let name = type_name_of(dummy_fn);
        &name[..name.len() - DUMMY_FN_EXTRA_SIZE]
    }};
}

// CFI directives cannot be used if neither debuginfo nor panic=unwind is enabled.
// We don't have an easy way to check the former, so just check based on panic strategy.
#[cfg(panic = "abort")]
macro_rules! maybe_cfi {
    ($x: literal) => {
        ""
    };
}

#[cfg(panic = "unwind")]
macro_rules! maybe_cfi {
    ($x: literal) => {
        $x
    };
}

pub(crate) use maybe_cfi;
