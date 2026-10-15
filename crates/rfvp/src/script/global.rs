use alloc::vec::Vec;
#[cfg(feature = "no_std")]
use core::cell::UnsafeCell;
#[cfg(feature = "no_std")]
use core::ops::{Deref, DerefMut};
#[cfg(not(feature = "no_std"))]
use std::sync::Mutex;

use crate::script::Variant;
#[cfg(feature = "no_std")]
use crate::utils::stable_hash::StableHashMap;
#[cfg(not(feature = "no_std"))]
use crate::utils::stable_hash::StableHashMap;
use serde::{Deserialize, Serialize};

/// Global variables
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Global {
    global_table: StableHashMap<u16, Variant>,
    none_volatile_count: u16,
    volatile_count: u16,
}

impl Global {
    pub fn new() -> Self {
        Global {
            global_table: StableHashMap::default(),
            none_volatile_count: 0,
            volatile_count: 0,
        }
    }

    pub fn get(&self, key: u16) -> Option<&Variant> {
        self.global_table.get(&key)
    }

    pub fn get_mut(&mut self, key: u16) -> Option<&mut Variant> {
        self.global_table.get_mut(&key)
    }

    pub fn set(&mut self, key: u16, value: Variant) {
        // The PlayStation's globals are the compiled script's (crate::aot).
        #[cfg(feature = "psx_aot")]
        {
            crate::aot::set_global_variant(key as usize, value);
            return;
        }
        #[allow(unreachable_code)]
        // The PlayStation keeps no entries for Nil (absent reads as Nil).
        #[cfg(feature = "psx")]
        if value.is_nil() {
            self.global_table.remove(&key);
            return;
        }
        self.global_table.insert(key, value);
    }

    /// The global `key` to write into: created (Nil) when it is one of the script's globals and
    /// has no entry yet.
    pub fn slot_mut(&mut self, key: u16) -> Option<&mut Variant> {
        if (key as u32) < self.none_volatile_count as u32 + self.volatile_count as u32 {
            Some(self.global_table.entry(key).or_insert(Variant::Nil))
        } else {
            self.global_table.get_mut(&key)
        }
    }

    pub fn init_with(&mut self, none_volatile: u16, volatile: u16) {
        self.none_volatile_count = none_volatile;
        self.volatile_count = volatile;

        // Every global starts as Nil. The PlayStation stores none of them until they are set
        // (thousands of Nil entries are 180 KiB of its 2 MiB).
        #[cfg(not(feature = "psx"))]
        for i in 0..none_volatile + volatile {
            self.global_table.insert(i, Variant::Nil);
        }
        #[cfg(feature = "psx")]
        self.global_table.clear();
    }

    pub fn get_int_var(&self, key: u16) -> i32 {
        let key = key + self.none_volatile_count;
        #[cfg(feature = "psx_aot")]
        return crate::aot::global_int(key as usize).unwrap_or(0);
        #[allow(unreachable_code)]
        if let Some(Variant::Int(val)) = self.global_table.get(&key) {
            return *val;
        }
        0
    }

    /// Global `key`'s value (Nil when unset).
    fn value(&self, key: u16) -> Variant {
        #[cfg(feature = "psx_aot")]
        return crate::aot::global_variant(key as usize);
        #[cfg(not(feature = "psx_aot"))]
        self.global_table.get(&key).cloned().unwrap_or(Variant::Nil)
    }

    pub fn non_volatile_count(&self) -> u16 {
        self.none_volatile_count
    }

    pub fn volatile_count(&self) -> u16 {
        self.volatile_count
    }

    /// Snapshot the first HCB global region. The original engine serializes
    /// this region as part of normal save slots.
    pub fn snapshot_non_volatile(&self) -> Vec<Variant> {
        let mut out: Vec<Variant> = Vec::with_capacity(self.none_volatile_count as usize);
        for i in 0..self.none_volatile_count {
            out.push(self.value(i));
        }
        out
    }

    /// The non-volatile globals as `snapshot_non_volatile` would give them, serialized straight
    /// from the table (no dense copy: on the PlayStation that copy alone is 40 KiB).
    pub fn non_volatile_view(&self) -> DenseGlobals<'_> {
        DenseGlobals { global: self, base: 0, count: self.none_volatile_count }
    }

    pub fn restore_non_volatile(&mut self, vals: &[Variant]) {
        let n = self.none_volatile_count as usize;
        let take = vals.len().min(n);
        for i in 0..take {
            self.set(i as u16, vals[i].clone());
        }
        // Missing entries remain unchanged.
    }

    /// Snapshot the second HCB global region. Despite the historical rfvp
    /// name, the original engine serializes this region in `save/save.bin`.
    pub fn snapshot_volatile_globals(&self) -> Vec<Variant> {
        let mut out: Vec<Variant> = Vec::with_capacity(self.volatile_count as usize);
        let base = self.none_volatile_count;
        for i in 0..self.volatile_count {
            out.push(self.value(base.saturating_add(i)));
        }
        out
    }

    pub fn restore_volatile_globals(
        &mut self,
        expected_non_volatile: u16,
        expected_volatile: u16,
        vars: &[Variant],
    ) {
        let base = self.none_volatile_count;

        if expected_non_volatile != self.none_volatile_count
            || expected_volatile != self.volatile_count
        {
            log::warn!(
            "GlobalSaveData: global counts mismatch: saved non_volatile={} volatile={} but current non_volatile={} volatile={}",
            expected_non_volatile,
            expected_volatile,
            self.none_volatile_count,
            self.volatile_count
        );
        }

        let n = vars.len().min(self.volatile_count as usize);
        for i in 0..n {
            let key = base.saturating_add(i as u16);
            self.set(key, vars[i].clone());
        }
    }
}

#[cfg(not(feature = "no_std"))]
lazy_static::lazy_static! {
    pub static ref GLOBAL: Mutex<Global> = Mutex::new(Global::new());
}

#[cfg(feature = "no_std")]
pub static GLOBAL: NoStdGlobal = NoStdGlobal::new();

#[cfg(feature = "no_std")]
pub struct NoStdGlobal {
    inner: UnsafeCell<Global>,
}

#[cfg(feature = "no_std")]
pub struct NoStdGlobalGuard<'a> {
    inner: &'a mut Global,
}

#[cfg(feature = "no_std")]
unsafe impl Sync for NoStdGlobal {}

#[cfg(feature = "no_std")]
impl NoStdGlobal {
    pub const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(Global {
                global_table: StableHashMap::new(),
                none_volatile_count: 0,
                volatile_count: 0,
            }),
        }
    }

    pub fn lock(&self) -> Result<NoStdGlobalGuard<'_>, ()> {
        Ok(NoStdGlobalGuard {
            inner: unsafe { &mut *self.inner.get() },
        })
    }
}

#[cfg(feature = "no_std")]
impl Deref for NoStdGlobalGuard<'_> {
    type Target = Global;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

#[cfg(feature = "no_std")]
impl DerefMut for NoStdGlobalGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

pub fn get_int_var(key: u16) -> i32 {
    GLOBAL.lock().unwrap().get_int_var(key)
}

/// A run of globals that serializes as a `Vec<Variant>` (missing ones as `Nil`).
pub struct DenseGlobals<'a> {
    global: &'a Global,
    base: u16,
    count: u16,
}

impl Serialize for DenseGlobals<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[cfg(feature = "psx_aot")]
        return serializer.collect_seq((0..self.count).map(|i| self.global.value(self.base.saturating_add(i))));
        #[cfg(not(feature = "psx_aot"))]
        {
            let nil = Variant::Nil;
            serializer.collect_seq(
                (0..self.count).map(|i| self.global.global_table.get(&self.base.saturating_add(i)).unwrap_or(&nil)),
            )
        }
    }
}
