//! TIPC ("tiny IPC") protocol support

use super::*;

/// Represents special TIPC command types
///
/// Note that regular/"Request" commands use `16 + <request-id>` as their command type
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum CommandType {
    Invalid = 0,
    CloseSession = 15,
}

pub mod client;

pub mod server;
