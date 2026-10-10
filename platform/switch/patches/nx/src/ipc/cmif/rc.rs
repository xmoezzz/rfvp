pub const RESULT_MODULE: u32 = 10;

result_define_group!(RESULT_MODULE => {
    InvalidHeaderSize: 202,
    InvalidInputHeader: 211,
    InvalidOutputHeader: 212,
    InvalidCommandRequestId: 221,
    InvalidInObjectCount: 235,
    InvalidOutObjectCount: 236
});
