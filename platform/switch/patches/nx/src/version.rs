//! System version utils

use crate::sync;
use core::cmp;
use core::fmt;

/// Represents a version with major, minor and micro components
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Version {
    /// The major component
    pub major: u8,
    /// The minor component
    pub minor: u8,
    /// The micro component
    pub micro: u8,
}

unsafe impl Sync for Version {}

impl Version {
    /// Creates an empty [`Version`] (with value `0.0.0`)
    #[inline]
    pub const fn empty() -> Self {
        Self {
            major: 0,
            minor: 0,
            micro: 0,
        }
    }

    /// Creates a [`Version`] with the supplied components
    ///
    /// # Arguments
    ///
    /// * `major`: The major component
    /// * `minor`: The minor component
    /// * `micro`: The micro component
    #[inline]
    pub const fn new(major: u8, minor: u8, micro: u8) -> Self {
        Self {
            major,
            minor,
            micro,
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        match self.major.cmp(&other.major) {
            cmp::Ordering::Equal => {}
            other => return other,
        };
        match self.minor.cmp(&other.minor) {
            cmp::Ordering::Equal => {}
            other => return other,
        };

        self.micro.cmp(&other.micro)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.micro)
    }
}

/// Represents an interval between versions, being optionally limited on both sides
///
/// An interval limited on both sides is, for example, `1.0.0-5.1.0` (inclusive)
///
/// An interval limited on one side is, for example, `*-3.0.0` (any version lower or equal to `3.0.0`) or `2.3.0-*` (any version higher or equal to `2.3.0`)
pub struct VersionInterval {
    min: Option<Version>,
    max: Option<Version>,
}

impl VersionInterval {
    /// Creates a non-limited [`VersionInterval`], essentially an interval allowing any versions
    #[inline]
    pub const fn all() -> Self {
        Self {
            min: None,
            max: None,
        }
    }

    /// Creates a left-limited [`VersionInterval`], including any version higher or equal to `min`
    ///
    /// # Arguments
    ///
    /// * `min`: The minimum [`Version`] limiting the interval
    #[inline]
    pub const fn from(min: Version) -> Self {
        Self {
            min: Some(min),
            max: None,
        }
    }

    /// Creates a right-limited [`VersionInterval`], including any version lower or equal to `max`
    ///
    /// # Arguments
    ///
    /// * `max`: The maximum [`Version`] limiting the interval
    #[inline]
    pub const fn to(max: Version) -> Self {
        Self {
            min: None,
            max: Some(max),
        }
    }

    /// Creates a limited [`VersionInterval`], including any version between `min` and `max` (inclusive)
    ///
    /// # Arguments
    ///
    /// * `min`: The minimum [`Version`] limiting the interval
    /// * `max`: The maximum [`Version`] limiting the interval
    #[inline]
    pub const fn from_to(min: Version, max: Version) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
        }
    }

    /// Returns whether `ver` is contained in the interval
    ///
    /// # Arguments
    ///
    /// * `ver`: The [`Version`] to check
    pub fn contains(&self, ver: Version) -> bool {
        if let Some(min_v) = self.min
            && ver < min_v
        {
            return false;
        }

        if let Some(max_v) = self.max
            && ver > max_v
        {
            return false;
        }

        true
    }
}

static G_VERSION: sync::Mutex<Version> = sync::Mutex::new(Version::empty());

/// Sets the global [`Version`], used in the library as the system [`Version`]
///
/// This is used on [`rrt0`][`crate::rrt0`] to set the actual system version, and shouldn't be used for other purposes unless you really know what you're doing
///
/// # Arguments
///
/// * `ver`: The system [`Version`] to set globally for the library
///
/// # Safety
///
/// This is automatically called in the runtime set up, and should not be modified by consuming crates.
pub unsafe fn set_version(ver: Version) {
    G_VERSION.set(ver);
}

/// Gets the global library value for the system [`Version`]
///
/// This value is set on [`rrt0`][`crate::rrt0`] to the actual system version
pub fn get_version() -> Version {
    G_VERSION.get_val()
}
