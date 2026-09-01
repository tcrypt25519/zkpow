//! Minimal Bitcoin P2P client.

use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
};

use bitcoin::{
    block::Header as BitcoinHeader,
    consensus::{encode, Decodable, Encodable},
    hashes::Hash,
};
use zkpow_core::{BlockHash, BlockTimestamp, CompactTarget, Header as CoreHeader};

use crate::types::{Header, Network};

/// Magic bytes for Bitcoin mainnet.
const MAINNET_MAGIC: [u8; 4] = [0xF9, 0xBE, 0xB4, 0xD9];

/// Bitcoin P2P protocol version.
const PROTOCOL_VERSION: u32 = 70015;

/// Our node's services (none, we're a headers-only client).
const SERVICES: u64 = 0;

/// User agent string.
const USER_AGENT: &str = "/zkpow-cli:0.1.0/";

/// Size of the command field in a Bitcoin P2P message frame.
const COMMAND_SIZE: usize = 12;

/// A Bitcoin P2P network message command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Version,
    Verack,
    SendHeaders,
    GetHeaders,
    Headers,
    // Unsolicited messages that we consume and discard.
    SendCmpct,
    Addr,
    AddrV2,
    Ping,
    Pong,
    FeeFilter,
    SendAddrV2,
    Unknown,
}

impl Command {
    fn as_bytes(&self) -> [u8; COMMAND_SIZE] {
        let mut buf = [0u8; COMMAND_SIZE];
        let name = self.name_bytes();
        buf[..name.len()].copy_from_slice(name);
        buf
    }

    fn name_bytes(&self) -> &'static [u8] {
        match self {
            Command::Version => b"version",
            Command::Verack => b"verack",
            Command::SendHeaders => b"sendheaders",
            Command::GetHeaders => b"getheaders",
            Command::Headers => b"headers",
            Command::SendCmpct => b"sendcmpct",
            Command::Addr => b"addr",
            Command::AddrV2 => b"addrv2",
            Command::Ping => b"ping",
            Command::Pong => b"pong",
            Command::FeeFilter => b"feefilter",
            Command::SendAddrV2 => b"sendaddrv2",
            Command::Unknown => b"unknown",
        }
    }

    fn from_wire(buf: &[u8; COMMAND_SIZE]) -> Self {
        let len = buf.iter().position(|&b| b == 0).unwrap_or(COMMAND_SIZE);
        let name = std::str::from_utf8(&buf[..len]);
        match name {
            Ok("version") => Command::Version,
            Ok("verack") => Command::Verack,
            Ok("sendheaders") => Command::SendHeaders,
            Ok("getheaders") => Command::GetHeaders,
            Ok("headers") => Command::Headers,
            Ok("sendcmpct") => Command::SendCmpct,
            Ok("addr") => Command::Addr,
            Ok("addrv2") => Command::AddrV2,
            Ok("ping") => Command::Ping,
            Ok("pong") => Command::Pong,
            Ok("feefilter") => Command::FeeFilter,
            Ok("sendaddrv2") => Command::SendAddrV2,
            _ => Command::Unknown,
        }
    }
}

/// Simplified version message structure.
#[derive(Debug)]
pub struct VersionMessage {
    pub version:           i32,
    #[allow(dead_code)]
    pub services:          u64,
    #[allow(dead_code)]
    pub timestamp:         i64,
    #[allow(dead_code)]
    pub receiver_services: u64,
    #[allow(dead_code)]
    pub sender_services:   u64,
    #[allow(dead_code)]
    pub nonce:             u64,
    pub user_agent:        String,
    pub starting_height:   i32,
}

impl Decodable for VersionMessage {
    fn consensus_decode<D: bitcoin::io::Read + ?Sized>(d: &mut D) -> std::result::Result<Self, encode::Error> {
        use bitcoin::io::Read as BitcoinRead;
        let version = Decodable::consensus_decode(d)?;
        let services = Decodable::consensus_decode(d)?;
        let timestamp = Decodable::consensus_decode(d)?;
        let receiver_services = Decodable::consensus_decode(d)?;

        let mut _receiver_addr = [0u8; 18];
        BitcoinRead::read_exact(d, &mut _receiver_addr)?;

        let sender_services = Decodable::consensus_decode(d)?;

        let mut _sender_addr = [0u8; 18];
        BitcoinRead::read_exact(d, &mut _sender_addr)?;

        let nonce = Decodable::consensus_decode(d)?;

        let ua_len: encode::VarInt = Decodable::consensus_decode(d)?;
        let mut ua_buf = vec![0u8; ua_len.0 as usize];
        BitcoinRead::read_exact(d, &mut ua_buf)?;
        let user_agent = String::from_utf8_lossy(&ua_buf).to_string();

        let starting_height = Decodable::consensus_decode(d)?;

        // Relay flag (1 byte), present since protocol version 70001.
        let mut _relay = [0u8; 1];
        let _ = BitcoinRead::read_exact(d, &mut _relay);

        Ok(VersionMessage {
            version,
            services,
            timestamp,
            receiver_services,
            sender_services,
            nonce,
            user_agent,
            starting_height,
        })
    }
}

/// A P2P connection to a Bitcoin peer.
pub struct PeerConnection {
    stream:        TcpStream,
    network_magic: [u8; 4],
}

impl PeerConnection {
    pub fn connect(addr: SocketAddr, network: Network, start_height: i32) -> std::io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true)?;

        let network_magic = match network {
            Network::Mainnet => MAINNET_MAGIC,
            Network::Testnet => [0x0B, 0x11, 0x09, 0x07],
            Network::Regtest => [0xFA, 0xBF, 0xB5, 0xDA],
        };

        let mut conn = Self { stream, network_magic };
        conn.handshake(start_height)?;
        Ok(conn)
    }

    fn handshake(&mut self, start_height: i32) -> std::io::Result<()> {
        self.send_version(start_height)?;
        let (_their_version, _their_start_height) = self.read_version()?;
        self.send_verack()?;
        self.read_verack()?;
        self.send_sendheaders()?;
        Ok(())
    }

    fn send_version(&mut self, start_height: i32) -> std::io::Result<()> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

        let mut payload = Vec::new();
        payload.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        payload.extend_from_slice(&SERVICES.to_le_bytes());
        payload.extend_from_slice(&timestamp.to_le_bytes());
        payload.extend_from_slice(&SERVICES.to_le_bytes());
        payload.extend_from_slice(&[0u8; 18]);
        payload.extend_from_slice(&SERVICES.to_le_bytes());
        payload.extend_from_slice(&[0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0, 0, 0, 0]);
        payload.extend_from_slice(&0u16.to_le_bytes());
        payload.extend_from_slice(&rand::random::<u64>().to_le_bytes());
        let ua_bytes = USER_AGENT.as_bytes();
        encode::VarInt(ua_bytes.len() as u64).consensus_encode(&mut payload)?;
        payload.extend_from_slice(ua_bytes);
        payload.extend_from_slice(&start_height.to_le_bytes());
        payload.push(0);

        self.write_message(Command::Version, &payload)
    }

    fn read_version(&mut self) -> std::io::Result<(VersionMessage, i32)> {
        let payload = self.read_message_of_type(Command::Version)?;
        let version_msg: VersionMessage = encode::deserialize(&payload).map_err(io_error)?;
        let start_height = version_msg.starting_height;

        tracing::info!(
            peer_version = version_msg.version,
            user_agent = %version_msg.user_agent,
            start_height = start_height,
            "Received version message from peer"
        );

        Ok((version_msg, start_height))
    }

    fn send_verack(&mut self) -> std::io::Result<()> {
        self.write_message(Command::Verack, &[])
    }

    fn read_verack(&mut self) -> std::io::Result<()> {
        self.read_message_of_type(Command::Verack)?;
        Ok(())
    }

    fn send_sendheaders(&mut self) -> std::io::Result<()> {
        self.write_message(Command::SendHeaders, &[])
    }

    pub fn send_getheaders(&mut self, locators: &[BlockHash], stop_hash: BlockHash) -> std::io::Result<()> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&PROTOCOL_VERSION.to_le_bytes());
        encode::VarInt(locators.len() as u64).consensus_encode(&mut payload)?;
        for hash in locators {
            payload.extend_from_slice(hash.as_inner());
        }
        payload.extend_from_slice(stop_hash.as_inner());
        self.write_message(Command::GetHeaders, &payload)
    }

    /// Read a `headers` message and return validated headers using our own types.
    pub fn read_headers(&mut self) -> std::io::Result<Vec<Header>> {
        let payload = self.read_message_of_type(Command::Headers)?;
        let mut cursor = std::io::Cursor::new(payload);
        let count: encode::VarInt = Decodable::consensus_decode(&mut cursor).map_err(io_error)?;

        let mut headers = Vec::with_capacity(count.0 as usize);
        for _ in 0..count.0 {
            let bh: BitcoinHeader = Decodable::consensus_decode(&mut cursor).map_err(io_error)?;
            let _tx_count: encode::VarInt = Decodable::consensus_decode(&mut cursor).map_err(io_error)?;
            headers.push(Header::new(bitcoin_to_core(bh)));
        }

        tracing::debug!(count = headers.len(), "Received headers from peer");
        Ok(headers)
    }

    fn write_message(&mut self, command: Command, payload: &[u8]) -> std::io::Result<()> {
        let mut buf = Vec::with_capacity(24 + payload.len());
        buf.extend_from_slice(&self.network_magic);
        buf.extend_from_slice(&command.as_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        let checksum = create_checksum(payload);
        buf.extend_from_slice(&checksum);
        buf.extend_from_slice(payload);
        self.stream.write_all(&buf)
    }

    fn read_message_of_type(&mut self, expected: Command) -> std::io::Result<Vec<u8>> {
        loop {
            let (command, payload) = self.read_message()?;
            if command == expected {
                return Ok(payload);
            }
            tracing::debug!(?command, ?expected, "Skipping unsolicited message");
        }
    }

    fn read_message(&mut self) -> std::io::Result<(Command, Vec<u8>)> {
        let mut magic = [0u8; 4];
        self.stream.read_exact(&mut magic)?;
        if magic != self.network_magic {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid magic bytes: {:02x?} (expected {:02x?})", magic, self.network_magic),
            ));
        }

        let mut cmd_buf = [0u8; COMMAND_SIZE];
        self.stream.read_exact(&mut cmd_buf)?;
        let command = Command::from_wire(&cmd_buf);

        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf)?;
        let payload_len = u32::from_le_bytes(len_buf) as usize;

        let mut checksum_buf = [0u8; 4];
        self.stream.read_exact(&mut checksum_buf)?;

        let mut payload = vec![0u8; payload_len];
        if payload_len > 0 {
            self.stream.read_exact(&mut payload)?;
        }

        if checksum_buf != create_checksum(&payload) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Checksum mismatch for message {:?}", command),
            ));
        }

        Ok((command, payload))
    }
}

#[must_use]
pub fn create_checksum(data: &[u8]) -> [u8; 4] {
    use sha2::{Digest, Sha256};
    let first: [u8; 32] = Sha256::digest(data).into();
    let second: [u8; 32] = Sha256::digest(first).into();
    second[..4].try_into().expect("should fit in [u8; 4]")
}

/// Convert a bitcoin-crate header (decoded from wire) to our core type.
fn bitcoin_to_core(bh: BitcoinHeader) -> CoreHeader {
    CoreHeader {
        version:        bh.version.to_consensus() as u32,
        prev_blockhash: BlockHash::new(bh.prev_blockhash.to_raw_hash().to_byte_array()),
        merkle_root:    bh.merkle_root.to_raw_hash().to_byte_array(),
        timestamp:      BlockTimestamp::new(bh.time),
        compact_target: CompactTarget::new(bh.bits.to_consensus()),
        nonce:          bh.nonce,
    }
}

/// Simplified version message structure.
fn io_error(e: encode::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
}
