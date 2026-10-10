pub const RESULT_MODULE: u32 = 115;

result_define_group!(RESULT_MODULE => {
    DeviceNotFound: 64,
    NeedRestart: 96,
    AreaNeedsToBeCreated: 128,
    AccessIdMismatch: 152,
    AreaAlreadyCreated: 168
});
