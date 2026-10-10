#[macro_export]
macro_rules! acquire {
    ($x:expr) => {
        ::core::sync::atomic::fence(Acquire)
    };
}
