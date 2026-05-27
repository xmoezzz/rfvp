#[cfg(feature = "no_std")]
use alloc::collections::{BTreeMap, BTreeSet};
#[cfg(all(not(feature = "no_std"), target_os = "uefi"))]
use std::collections::hash_map::DefaultHasher;
#[cfg(not(feature = "no_std"))]
use std::collections::{HashMap, HashSet};
#[cfg(all(not(feature = "no_std"), target_os = "uefi"))]
use std::hash::BuildHasherDefault;

#[cfg(feature = "no_std")]
pub type StableHashMap<K, V> = BTreeMap<K, V>;

#[cfg(feature = "no_std")]
pub type StableHashSet<T> = BTreeSet<T>;

#[cfg(all(not(feature = "no_std"), target_os = "uefi"))]
pub type StableHashMap<K, V> = HashMap<K, V, BuildHasherDefault<DefaultHasher>>;

#[cfg(all(not(feature = "no_std"), target_os = "uefi"))]
pub type StableHashSet<T> = HashSet<T, BuildHasherDefault<DefaultHasher>>;

#[cfg(all(not(feature = "no_std"), not(target_os = "uefi")))]
pub type StableHashMap<K, V> = HashMap<K, V>;

#[cfg(all(not(feature = "no_std"), not(target_os = "uefi")))]
pub type StableHashSet<T> = HashSet<T>;
