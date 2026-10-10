use crate::rc;

/// Result Submodule ID for the parent module
pub const RESULT_SUBMODULE: u32 = 600;

result_define_subgroup!(rc::RESULT_MODULE, RESULT_SUBMODULE => {
    CopyHandlesFull: 1,
    MoveHandlesFull: 2,
    DomainObjectsFull: 3,
    InvalidDomainObject: 4,
    PointerSizesFull: 5,
    SendStaticsFull: 6,
    ReceiveStaticsFull: 7,
    SendBuffersFull: 8,
    ReceiveBuffersFull: 9,
    ExchangeBuffersFull: 10,
    InvalidSendStaticCount: 11,
    InvalidReceiveStaticCount: 12,
    InvalidSendBufferCount: 13,
    InvalidReceiveBufferCount: 14,
    InvalidExchangeBufferCount: 15,
    InvalidBufferAttributes: 16,
    InvalidProtocol: 17,
    InvalidBufferPointer: 18
});
