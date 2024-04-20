mod slice;
pub use slice::{ParallelSlice, ParallelSliceMut};

mod task;
pub use task::Task;

#[cfg(not(target_arch = "wasm32"))]
mod task_pool;
#[cfg(not(target_arch = "wasm32"))]
pub use task_pool::{Scope, TaskPool, TaskPoolBuilder};

#[cfg(target_arch = "wasm32")]
mod single_threaded_task_pool;
#[cfg(target_arch = "wasm32")]
pub use single_threaded_task_pool::{Scope, TaskPool, TaskPoolBuilder, ThreadExecutor};

mod usages;
#[cfg(not(target_arch = "wasm32"))]
pub use usages::tick_global_task_pools_on_main_thread;
pub use usages::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool};

#[cfg(not(target_arch = "wasm32"))]
mod thread_executor;
#[cfg(not(target_arch = "wasm32"))]
pub use thread_executor::{ThreadExecutor, ThreadExecutorTicker};

mod iter;
pub use iter::ParallelIterator;

#[allow(missing_docs)]
pub mod prelude {
    #[doc(hidden)]
    pub use crate::{
        iter::ParallelIterator,
        slice::{ParallelSlice, ParallelSliceMut},
        usages::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool},
    };
}

use std::num::NonZeroUsize;

use tracing::debug;

/// Gets the logical CPU core count available to the current process.
///
/// This is identical to [`std::thread::available_parallelism`], except
/// it will return a default value of 1 if it internally errors out.
///
/// This will always return at least 1.
pub fn available_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(NonZeroUsize::get)
        .unwrap_or(1)
}

/// Initialize the global task pools.
///
/// Currently the AsyncComputeTaskPool and IoTaskPool are initialized. This may change in the future.
pub fn create_task_pools() {
    // bevy params:
    // TaskPoolOptions {
    //     // By default, use however many cores are available on the system
    //     min_total_threads: 1,
    //     max_total_threads: std::usize::MAX,
    //
    //     // Use 25% of cores for IO, at least 1, no more than 4
    //     io: TaskPoolThreadAssignmentPolicy {
    //         min_threads: 1,
    //         max_threads: 4,
    //         percent: 0.25,
    //     },
    //
    //     // Use 25% of cores for async compute, at least 1, no more than 4
    //     async_compute: TaskPoolThreadAssignmentPolicy {
    //         min_threads: 1,
    //         max_threads: 4,
    //         percent: 0.25,
    //     },
    //
    //     // Use all remaining cores for compute (at least 1)
    //     compute: TaskPoolThreadAssignmentPolicy {
    //         min_threads: 1,
    //         max_threads: std::usize::MAX,
    //         percent: 1.0, // This 1.0 here means "whatever is left over"
    //     },
    // }

    let total_threads = available_parallelism().clamp(1, usize::MAX);
    debug!("Assigning {} cores to default task pools", total_threads);

    let mut remaining_threads = total_threads;

    fn get_number_of_threads(
        percent: f32,
        min_threads: usize,
        max_threads: usize,
        remaining_threads: usize,
        total_threads: usize,
    ) -> usize {
        let mut desired = (total_threads as f32 * percent).round() as usize;

        // Limit ourselves to the number of cores available
        desired = desired.min(remaining_threads);

        // Clamp by min_threads, max_threads. (This may result in us using more threads than are
        // available, this is intended. An example case where this might happen is a device with
        // <= 2 threads.
        desired.clamp(min_threads, max_threads)
    }

    {
        // Determine the number of IO threads we will use
        let io_threads = get_number_of_threads(0.25, 1, 4, remaining_threads, total_threads);

        debug!("IO Threads: {}", io_threads);
        remaining_threads = remaining_threads.saturating_sub(io_threads);

        IoTaskPool::init(|| {
            TaskPoolBuilder::default()
                .num_threads(io_threads)
                .thread_name("IO Task Pool".to_string())
                .build()
        });
    }

    {
        // Use the rest for async compute threads
        let async_compute_threads = remaining_threads;
        // get_number_of_threads(0.25, 1, 4, remaining_threads, total_threads);

        debug!("Async Compute Threads: {}", async_compute_threads);
        remaining_threads = remaining_threads.saturating_sub(async_compute_threads);

        AsyncComputeTaskPool::init(|| {
            TaskPoolBuilder::default()
                .num_threads(async_compute_threads)
                .thread_name("Async Compute Task Pool".to_string())
                .build()
        });
    }

    // do not initialize the compute task pool, we do not use it (at least for now)
    debug!("Remaining Threads: {}", remaining_threads);
}
