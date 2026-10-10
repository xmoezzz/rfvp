pub const RESULT_MODULE: u32 = 17;

result_define_group!(RESULT_MODULE => {
    NotInitialized: 1,
    InvalidSocketString: 2,
    InvalidSockAddr:3,
    InvalidTimeout: 4
});
