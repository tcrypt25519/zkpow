//! Peer discovery and management via DNS seeds.

use std::{
    collections::{HashMap, HashSet},
    fmt,
    net::{SocketAddr, ToSocketAddrs},
    time::SystemTime,
};

use rand::seq::IndexedRandom;




/// Known Bitcoin mainnet DNS seeds.
const DNS_SEEDS: &[&str] = &[
    "seed.bitcoin.sipa.be",
    "dnsseed.bluematt.me",
    "dnsseed.bitcoin.dashjr.org",
    "seed.bitcoinstats.com",
    "seed.bitcoin.jonasschnelli.ch",
    "seed.btc.petertodd.org",
    "seed.bitcoin.sprovoost.nl",
    "dnsseed.emzy.de",
];

/// Default P2P port for Bitcoin mainnet.
const DEFAULT_PORT: u16 = 8333;

/// Lifecycle status of a known peer address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PeerStatus {
    /// Discovered from a DNS seed but never contacted.
    Unknown,
    /// Currently connected and communicating.
    Connected,
    /// Connection attempt failed. The peer did not respond.
    Unreachable,
    /// Was connected but the peer disconnected.
    Disconnected,
    /// Was connected but we disconnected due to excessive latency.
    TooSlow,
}

impl fmt::Display for PeerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeerStatus::Unknown => write!(f, "unknown"),
            PeerStatus::Connected => write!(f, "connected"),
            PeerStatus::Unreachable => write!(f, "unreachable"),
            PeerStatus::Disconnected => write!(f, "disconnected"),
            PeerStatus::TooSlow => write!(f, "too_slow"),
        }
    }
}

/// A known peer address with its current status and a timestamp.
#[derive(Debug, Clone)]
pub struct PeerEntry {
    pub addr:   SocketAddr,
    pub status: PeerStatus,
    pub since:  SystemTime,
}

impl PeerEntry {
    fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            status: PeerStatus::Unknown,
            since: SystemTime::now(),
        }
    }

    fn transition(&mut self, status: PeerStatus) {
        self.status = status;
        self.since = SystemTime::now();
    }
}

/// A collection of discovered peers with status tracking.
#[derive(Default)]
pub struct PeerStore {
    entries: Vec<PeerEntry>,
}

impl PeerStore {
    /// Resolve all DNS seeds and populate the peer store.
    pub fn from_dns_seeds() -> std::io::Result<Self> {
        let mut seen = HashSet::new();
        let mut entries = Vec::new();

        for seed in DNS_SEEDS {
            let seed_addr = format!("{}:{}", seed, DEFAULT_PORT);
            match seed_addr.to_socket_addrs() {
                Ok(addrs) => {
                    let count = addrs
                        .filter(|a| match a.ip() {
                            std::net::IpAddr::V4(ip) => !ip.is_loopback() && !ip.is_private() && !ip.is_link_local(),
                            std::net::IpAddr::V6(ip) => !ip.is_loopback() && !ip.is_unspecified(),
                        })
                        .filter_map(|a| if seen.insert(a) { Some(PeerEntry::new(a)) } else { None })
                        .collect::<Vec<_>>();
                    tracing::debug!(seed = %seed, resolved = count.len(), "DNS seed resolved");
                    entries.extend(count);
                },
                Err(e) => {
                    tracing::warn!(seed = %seed, error = %e, "Failed to resolve DNS seed");
                },
            }
        }

        tracing::info!(peers = entries.len(), "Discovered peers from DNS seeds");
        Ok(Self { entries })
    }

    /// Pick a random peer with `Unknown` or `Disconnected` status.
    pub fn pick_candidate(&mut self, rng: &mut impl rand::Rng) -> Option<SocketAddr> {
        let candidates: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e.status, PeerStatus::Unknown | PeerStatus::Disconnected))
            .map(|(i, _)| i)
            .collect();

        if candidates.is_empty() {
            let fallback: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| matches!(e.status, PeerStatus::Unreachable | PeerStatus::TooSlow))
                .map(|(i, _)| i)
                .collect();

            if fallback.is_empty() {
                return None;
            }
            let idx = *fallback.choose(rng)?;
            Some(self.entries[idx].addr)
        } else {
            let idx = *candidates.choose(rng)?;
            Some(self.entries[idx].addr)
        }
    }

    pub fn mark_connected(&mut self, addr: &SocketAddr) {
        if let Some(entry) = self.find_mut(addr) {
            entry.transition(PeerStatus::Connected);
        }
    }

    pub fn mark_unreachable(&mut self, addr: &SocketAddr) {
        if let Some(entry) = self.find_mut(addr) {
            entry.transition(PeerStatus::Unreachable);
        }
    }

    pub fn mark_disconnected(&mut self, addr: &SocketAddr) {
        if let Some(entry) = self.find_mut(addr) {
            entry.transition(PeerStatus::Disconnected);
        }
    }

    pub fn mark_too_slow(&mut self, addr: &SocketAddr) {
        if let Some(entry) = self.find_mut(addr) {
            entry.transition(PeerStatus::TooSlow);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn status_summary(&self) -> HashMap<PeerStatus, usize> {
        let mut map = HashMap::new();
        for entry in &self.entries {
            *map.entry(entry.status).or_insert(0) += 1;
        }
        map
    }

    fn find_mut(&mut self, addr: &SocketAddr) -> Option<&mut PeerEntry> {
        self.entries.iter_mut().find(|e| &e.addr == addr)
    }
}
