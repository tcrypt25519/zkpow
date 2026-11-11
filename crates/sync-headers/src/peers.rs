//! Peer discovery and management via DNS seeds.

use std::{
    collections::{HashMap, HashSet},
    fmt,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    time::SystemTime,
};

use rand::seq::IndexedRandom;
use strum_macros::EnumString;

/// Known Bitcoin mainnet DNS seeds.
const DNS_SEEDS: &[&str] = &[
    "seed.bitcoin.sipa.be:8333",
    "dnsseed.bluematt.me:8333",
    "dnsseed.bitcoin.dashjr.org:8333",
    "seed.bitcoinstats.com:8333",
    "seed.bitcoin.jonasschnelli.ch:8333",
    "seed.btc.petertodd.org:8333",
    "seed.bitcoin.sprovoost.nl:8333",
    "dnsseed.emzy.de:8333",
];

/// Lifecycle status of a known peer address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString)]
#[strum(ascii_case_insensitive, serialize_all = "lowercase")]
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
        write!(f, "{}", self.to_string())
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

        let entries: Vec<PeerEntry> = DNS_SEEDS
            .iter()
            .filter_map(|s| {
                // Compute the addresses fort this seed; drop the seed if the resolution fails.
                let Ok(addrs) = s.to_socket_addrs() else {
                    tracing::warn!(seed = %s, error = %seeds.unwrap_err(), "Failed to resolve DNS seed");
                    return None;
                };

                // Filter out loopback, private, link-local addresses, and duplicates.
                // Return PeerEntry for each of the remaining addresses.
                Some(
                    addrs
                        .filter(|a| match a.ip() {
                            IpAddr::V4(ip) => !ip.is_loopback() && !ip.is_private() && !ip.is_link_local(),
                            IpAddr::V6(ip) => !ip.is_loopback() && !ip.is_unspecified(),
                        })
                        .filter_map(|a| seen.insert(a).then(|| PeerEntry::new(a)))
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect();

        tracing::info!(peers = entries.len(), "Discovered peers from DNS seeds");
        Ok(Self { entries })
    }

    /// Pick a random peer with `Unknown` or `Disconnected` status.
    pub fn pick_candidate(&mut self, rng: &mut impl rand::Rng) -> Option<SocketAddr> {
        self.entries
            .iter()
            .filter(|e| matches!(e.status, PeerStatus::Unknown | PeerStatus::Disconnected))
            .map(|e| e.addr)
            .collect::<Vec<_>>()
            .choose(rng)
            .copied()
    }

    pub fn add_peer(&mut self, addr: SocketAddr) {
        if self.find_mut(&addr).is_none() {
            self.entries.push(PeerEntry::new(addr));
        }
    }

    pub fn mark_status(&mut self, addr: &SocketAddr, status: PeerStatus) {
        self.find_mut(addr).map(|e| e.transition(status));
    }

    pub fn mark_connected(&mut self, addr: &SocketAddr) {
        self.mark_status(addr, PeerStatus::Connected);
    }

    pub fn mark_unreachable(&mut self, addr: &SocketAddr) {
        self.mark_status(addr, PeerStatus::Unreachable);
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
