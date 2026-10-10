#![macro_use]

/// Defines the default heap size of the current project
///
/// Must only be called once in a project
///
/// # Arguments
///
/// * `size`: The size passed to the user-configurable heap-initialization function.
///
/// # Examples
///
/// ```
/// alloc_set_default_heap_size!(0x13F00);
/// ```
#[macro_export]
macro_rules! alloc_set_default_heap_size {
    ($val:expr) => {
        #[unsafe(no_mangle)]
        #[used]
        #[unsafe(export_name = "__nx_mem_alloc_default_heap_size")]
        pub static DEFAULT_HEAP_SIZE: usize = $val;
    };
}
