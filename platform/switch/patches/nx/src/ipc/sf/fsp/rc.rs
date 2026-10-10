pub const RESULT_MODULE: u32 = 2;

result_define_group!(RESULT_MODULE => {
    PathNotFound: 1,
    PathAlreadyExists: 2
});
