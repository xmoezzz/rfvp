#[cfg(target_os = "uefi")]
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
#[cfg(target_os = "uefi")]
use std::hash::BuildHasherDefault;

#[cfg(target_os = "uefi")]
pub type StableHashMap<K, V> = HashMap<K, V, BuildHasherDefault<DefaultHasher>>;

#[cfg(target_os = "uefi")]
pub type StableHashSet<T> = HashSet<T, BuildHasherDefault<DefaultHasher>>;

#[cfg(not(target_os = "uefi"))]
pub type StableHashMap<K, V> = HashMap<K, V>;

#[cfg(not(target_os = "uefi"))]
pub type StableHashSet<T> = HashSet<T>;
