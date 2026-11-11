use std::time::Duration;

use strum::EnumCount;

use crate::Network;

/// Magic bytes for Bitcoin mainnet.
pub(crate) const MAGIC: [[u8; 4]; Network::COUNT] = [
    [0xF9, 0xBE, 0xB4, 0xD9], // mainnet
    [0x0B, 0x11, 0x09, 0x07], // testnet
    [0xFA, 0xBF, 0xB5, 0xDA], // regtest
];

/// Bitcoin P2P protocol version.
pub(crate) const VERSION: u32 = 70015;

/// Our node's services (none, we're a headers-only client).
pub(crate) const SERVICES: u64 = 0;

/// User agent string.
pub(crate) const USER_AGENT: &str = "/zkpow-cli:0.1.0/";

/// Size of the command field in a Bitcoin P2P message frame.
pub(crate) const COMMAND_SIZE: usize = 12;

pub(crate) const MAX_HEADERS_PER_MSG: usize = 2000;
pub(crate) const MAX_MESSAGE_SIZE: usize = 4_000_000;
pub(crate) const MAX_USER_AGENT_SIZE: u64 = 256;

pub(crate) const MAX_SKIPPED_MESSAGES: usize = 64;
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const IO_TIMEOUT: Duration = Duration::from_secs(30);
