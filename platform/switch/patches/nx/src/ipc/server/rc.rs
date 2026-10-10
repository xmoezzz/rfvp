use crate::rc;

/// Result Submodule ID for the parent module
pub const RESULT_SUBMODULE: u32 = 1300;

result_define_subgroup!(rc::RESULT_MODULE, RESULT_SUBMODULE => {
    ObjectIdAlreadyAllocated: 1,
    DomainNotFound: 2,
    InvalidCommandType: 3,
    InvalidDomainCommandType: 4,
    SignaledServerNotFound: 5,
    AlreadyDomain: 6
});
