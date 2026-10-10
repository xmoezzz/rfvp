//! Pseudo-RNG support

use alloc::sync::Arc;
pub use rand::{Rng, RngCore};

/// Represents a pseudo-RNG
use crate::ipc::sf::Buffer;
use crate::result::*;
use crate::service;
pub use crate::service::spl::{IRandomClient, RandomService};
use crate::sync::Mutex;

impl RngCore for RandomService {
    fn next_u32(&mut self) -> u32 {
        let mut data = [0; 4];
        self.generate_random_bytes(Buffer::from_mut_array(&mut data))
            .expect("Generating rand bytes should never fail");
        u32::from_ne_bytes(data)
    }

    fn next_u64(&mut self) -> u64 {
        let mut data = [0; 8];
        self.generate_random_bytes(Buffer::from_mut_array(&mut data))
            .expect("Generating rand bytes should never fail");
        u64::from_ne_bytes(data)
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        self.generate_random_bytes(Buffer::from_mut_array(dst))
            .expect("Generating rand bytes should never fail");
    }
}

// Global RNG source
static G_RNG: Mutex<Option<spl::SplCsrngGenerator>> = Mutex::new(None);

pub fn initialize() -> Result<()> {
    let mut guard = G_RNG.lock();
    if guard.is_none() {
        *guard = Some(spl::SplCsrngGenerator::new()?);
    }

    Ok(())
}

pub fn finalize() {
    *G_RNG.lock() = None;
}

#[inline]
pub fn get_rng() -> Result<spl::SplCsrngGenerator> {
    G_RNG
        .lock()
        .clone()
        .ok_or(nx::rc::ResultNotInitialized::make())
}

mod spl {
    use super::*;

    /// Represents a pseudo-RNG using [`spl`][`crate::service::spl`]'s [`RandomService`] interface
    #[derive(Clone)]
    pub struct SplCsrngGenerator {
        csrng: Arc<RandomService>,
    }

    impl SplCsrngGenerator {
        /// Creates a new [`SplCsrngGenerator`]
        pub fn new() -> Result<Self> {
            Ok(Self {
                csrng: Arc::new(service::new_service_object::<RandomService>()?),
            })
        }
    }

    impl RngCore for SplCsrngGenerator {
        fn next_u32(&mut self) -> u32 {
            let mut data = [0; 4];
            self.csrng
                .generate_random_bytes(Buffer::from_mut_array(&mut data))
                .expect("Generating rand bytes should never fail");
            u32::from_ne_bytes(data)
        }

        fn next_u64(&mut self) -> u64 {
            let mut data = [0; 8];
            self.csrng
                .generate_random_bytes(Buffer::from_mut_array(&mut data))
                .expect("Generating rand bytes should never fail");
            u64::from_ne_bytes(data)
        }

        fn fill_bytes(&mut self, dst: &mut [u8]) {
            self.csrng
                .generate_random_bytes(Buffer::from_mut_array(dst))
                .expect("Generating rand bytes should never fail");
        }
    }
}
