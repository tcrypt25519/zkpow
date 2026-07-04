//! Alchemy RPC header sync backend.
//!
//! Reads the block hashes already in the store's `headers` table, fetches each
//! block's detailed header from the Alchemy Bitcoin RPC, and writes them into a
//! separate `headers_alchemy` table. The entry point is [`sync`], re-exported
//! as [`crate::alchemy::sync`].

use std::{
    collections::{HashSet, VecDeque},
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use bitcoin::hashes::Hash;
use reqwest::Client;
use rusqlite::Connection;
use serde_json::{json, Value};
use tokio::time::sleep;

#[derive(Clone)]
struct RateLimiter {
    inner: Arc<Mutex<VecDeque<Instant>>>,
    max_requests: usize,
    window: Duration,
}

impl RateLimiter {
    fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(max_requests + 1))),
            max_requests,
            window,
        }
    }

    /// Block (async-sleep) until we can fire one more request inside the window.
    async fn acquire(&self) {
        loop {
            let sleep_for = {
                let mut q = self.inner.lock().unwrap();
                let now = Instant::now();

                // Drop timestamps older than the window.
                while q
                    .front()
                    .is_some_and(|t| now.duration_since(*t) >= self.window)
                {
                    q.pop_front();
                }

                if q.len() < self.max_requests {
                    // Slot available right now – stamp and return immediately.
                    q.push_back(now);
                    Duration::ZERO
                } else {
                    // Oldest stamp + window = when the next slot opens.
                    let oldest = *q.front().unwrap();
                    oldest + self.window - now
                }
            }; // lock released before sleeping

            if sleep_for.is_zero() {
                return;
            }
            sleep(sleep_for).await;
        }
    }
}

struct AlchemyHeader {
    block_hash: [u8; 32],
    prev: [u8; 32],
    merkle_root: [u8; 32],
    version: i64,
    timestamp: i64,
    n_bits: i64,
    nonce: i64,
    height: i64,
    chainwork: [u8; 32],
    median_time_past: i64,
}

struct AlchemyHeaderStore {
    conn: Connection,
}

impl AlchemyHeaderStore {
    fn open(db_path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;

            CREATE TABLE IF NOT EXISTS headers_alchemy (
                block_hash      BLOB PRIMARY KEY,
                prev            BLOB NOT NULL,
                merkle_root     BLOB NOT NULL,
                version         INTEGER NOT NULL,
                timestamp       INTEGER NOT NULL,
                n_bits          INTEGER NOT NULL,
                nonce           INTEGER NOT NULL,
                height          INTEGER NOT NULL,
                chainwork       BLOB NOT NULL,
                median_time_past INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_headers_alchemy_prev ON headers_alchemy(prev);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_headers_alchemy_height ON headers_alchemy(height);
            ",
        )?;
        Ok(Self { conn })
    }

    fn get_all_header_hashes(&self) -> rusqlite::Result<Vec<(i64, [u8; 32])>> {
        let mut stmt = self
            .conn
            .prepare("SELECT height, block_hash FROM headers ORDER BY height DESC")?;
        let rows = stmt.query_map([], |row| {
            let height: i64 = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let hash_bytes: [u8; 32] = blob.try_into().map_err(|_| {
                rusqlite::Error::InvalidColumnType(1, "blob".into(), rusqlite::types::Type::Blob)
            })?;
            Ok((height, hash_bytes))
        })?;
        let mut res = Vec::new();
        for r in rows {
            res.push(r?);
        }
        Ok(res)
    }

    fn get_fetched_hashes(&self) -> rusqlite::Result<HashSet<[u8; 32]>> {
        let mut stmt = self
            .conn
            .prepare("SELECT block_hash FROM headers_alchemy")?;
        let rows = stmt.query_map([], |row| {
            let blob: Vec<u8> = row.get(0)?;
            let hash_bytes: [u8; 32] = blob.try_into().map_err(|_| {
                rusqlite::Error::InvalidColumnType(0, "blob".into(), rusqlite::types::Type::Blob)
            })?;
            Ok(hash_bytes)
        })?;
        let mut res = HashSet::new();
        for r in rows {
            res.insert(r?);
        }
        Ok(res)
    }

    fn insert_headers_batch(&mut self, headers: &[AlchemyHeader]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        for header in headers {
            tx.execute(
                "INSERT OR IGNORE INTO headers_alchemy
                 (block_hash, prev, merkle_root, version, timestamp, n_bits, nonce, height, chainwork, median_time_past)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    header.block_hash,
                    header.prev,
                    header.merkle_root,
                    header.version,
                    header.timestamp,
                    header.n_bits,
                    header.nonce,
                    header.height,
                    header.chainwork,
                    header.median_time_past,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn parse_alchemy_header(result: &Value) -> anyhow::Result<AlchemyHeader> {
    let hash_str = result["hash"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing hash"))?;
    let hash = bitcoin::BlockHash::from_str(hash_str)?;

    let prev_hash = match result.get("previousblockhash").and_then(|v| v.as_str()) {
        Some(prev_str) => bitcoin::BlockHash::from_str(prev_str)?,
        None => bitcoin::BlockHash::all_zeros(),
    };

    let merkle_str = result["merkleroot"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing merkleroot"))?;
    let merkle_root = bitcoin::TxMerkleNode::from_str(merkle_str)?;

    let version = result["version"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("missing version"))?;
    let timestamp = result["time"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("missing time"))?;

    let bits_str = result["bits"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing bits"))?;
    let n_bits = u32::from_str_radix(bits_str, 16)?;

    let nonce = result["nonce"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("missing nonce"))?;
    let height = result["height"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("missing height"))?;

    let chainwork_str = result["chainwork"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing chainwork"))?;
    let chainwork_bytes = hex::decode(chainwork_str)?;
    if chainwork_bytes.len() != 32 {
        anyhow::bail!("invalid chainwork length: {}", chainwork_bytes.len());
    }

    let median_time_past = result["mediantime"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("missing mediantime"))?;

    Ok(AlchemyHeader {
        block_hash: hash.to_raw_hash().to_byte_array(),
        prev: prev_hash.to_raw_hash().to_byte_array(),
        merkle_root: merkle_root.to_raw_hash().to_byte_array(),
        version,
        timestamp,
        n_bits: n_bits as i64,
        nonce,
        height,
        chainwork: chainwork_bytes.try_into().unwrap(),
        median_time_past,
    })
}

async fn fetch_block(
    client: &Client,
    url: &str,
    limiter: &RateLimiter,
    height: i64,
    hash_bytes: [u8; 32],
) -> anyhow::Result<Value> {
    let hash = bitcoin::BlockHash::from_raw_hash(bitcoin::hashes::sha256d::Hash::from_byte_array(
        hash_bytes,
    ));
    let hash_hex = hash.to_string();

    let mut attempts = 0;
    let max_attempts = 5;
    let mut backoff = Duration::from_millis(500);

    loop {
        attempts += 1;
        limiter.acquire().await;

        let body = json!({
            "jsonrpc": "2.0",
            "id": height,
            "method": "getblockheader",
            "params": [hash_hex, true]
        });

        match client.post(url).json(&body).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<Value>().await {
                        Ok(json_resp) => {
                            if let Some(err) = json_resp.get("error") {
                                tracing::error!(
                                    "RPC error on attempt {} for block {} (hash {}): {:?}",
                                    attempts,
                                    height,
                                    hash_hex,
                                    err
                                );
                                if attempts >= max_attempts {
                                    anyhow::bail!("RPC error for block {height}: {err}");
                                }
                            } else if let Some(result) = json_resp.get("result") {
                                if !result.is_null() {
                                    return Ok(result.clone());
                                } else {
                                    tracing::warn!(
                                        "Result was null for block {} (hash {}) on attempt {}",
                                        height,
                                        hash_hex,
                                        attempts
                                    );
                                    if attempts >= max_attempts {
                                        anyhow::bail!("Null result for block {height}");
                                    }
                                }
                            } else {
                                tracing::warn!(
                                    "Missing result/error for block {} (hash {}) on attempt {}",
                                    height,
                                    hash_hex,
                                    attempts
                                );
                                if attempts >= max_attempts {
                                    anyhow::bail!("Missing result/error for block {height}");
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse response body as JSON on attempt {} for block {} (hash {}): {:?}", attempts, height, hash_hex, e);
                            if attempts >= max_attempts {
                                return Err(e.into());
                            }
                        }
                    }
                } else if resp.status().as_u16() == 429 {
                    tracing::warn!(
                        "Rate limited (429) on attempt {} for block {} (hash {})",
                        attempts,
                        height,
                        hash_hex
                    );
                    if attempts >= max_attempts {
                        anyhow::bail!("Too many rate limits (429) for block {height}");
                    }
                } else {
                    tracing::error!(
                        "Non-success status code {} on attempt {} for block {} (hash {})",
                        resp.status(),
                        attempts,
                        height,
                        hash_hex
                    );
                    if attempts >= max_attempts {
                        anyhow::bail!("Non-success status: {}", resp.status());
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    "Network error on attempt {} for block {} (hash {}): {:?}",
                    attempts,
                    height,
                    hash_hex,
                    e
                );
                if attempts >= max_attempts {
                    return Err(e.into());
                }
            }
        }

        // Apply backoff
        sleep(backoff).await;
        backoff *= 2;
    }
}

/// Fetch detailed block headers from Alchemy and store them in the
/// `headers_alchemy` table. Re-exported as [`crate::alchemy::sync`].
///
/// `max_headers` of 0 means unlimited; `rate_limit` is the maximum requests per
/// second and `concurrency` the number of in-flight requests. Requires the
/// `ALCHEMY_API_KEY` environment variable.
pub async fn sync(
    db: &str,
    max_headers: u64,
    rate_limit: usize,
    concurrency: usize,
) -> anyhow::Result<()> {
    let api_key =
        std::env::var("ALCHEMY_API_KEY").expect("Set ALCHEMY_API_KEY environment variable");
    let url = format!("https://bitcoin-mainnet.g.alchemy.com/v2/{}", api_key);

    tracing::info!(db = %db, "Opening database");
    let store = AlchemyHeaderStore::open(db)?;

    tracing::info!("Querying existing headers from DB...");
    let all_headers = store.get_all_header_hashes()?;
    tracing::info!("Found {} headers in headers table", all_headers.len());

    let fetched_hashes = store.get_fetched_hashes()?;
    tracing::info!(
        "Found {} headers already fetched in headers_alchemy table",
        fetched_hashes.len()
    );

    let mut to_fetch: Vec<(i64, [u8; 32])> = all_headers
        .into_iter()
        .filter(|(_, hash)| !fetched_hashes.contains(hash))
        .collect();

    tracing::info!("{} headers remaining to fetch from Alchemy", to_fetch.len());

    if to_fetch.is_empty() {
        tracing::info!("All headers are already fetched! Nothing to do.");
        return Ok(());
    }

    // Sort by height descending (newest first)
    to_fetch.sort_by_key(|(height, _)| -*height);

    if max_headers > 0 && to_fetch.len() > max_headers as usize {
        tracing::info!(
            "Limiting sync to the first {} headers (max-headers)",
            max_headers
        );
        to_fetch.truncate(max_headers as usize);
    }

    let total = to_fetch.len();
    let mut fetched = 0;
    let start = Instant::now();

    let client = Client::builder().timeout(Duration::from_secs(30)).build()?;

    // Dynamic rate limiter with a sliding 1-second window.
    let limiter = RateLimiter::new(rate_limit, Duration::from_secs(1));
    let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let store = Arc::new(Mutex::new(store));

    let mut handles = Vec::new();

    for (height, hash_bytes) in to_fetch {
        let permit = sem.clone().acquire_owned().await?;
        let client = client.clone();
        let url = url.clone();
        let limiter = limiter.clone();
        let store_clone = store.clone();

        handles.push(tokio::spawn(async move {
            let result = fetch_block(&client, &url, &limiter, height, hash_bytes).await;
            drop(permit);
            (height, result, store_clone)
        }));

        // Drain completed handles periodically to keep memory bounded and write in fast transactions.
        if handles.len() >= concurrency * 4 {
            let done: Vec<_> = handles.drain(..concurrency * 2).collect();
            let mut headers_to_insert = Vec::new();
            let mut last_height = 0;

            for h in done {
                let (height, res, _) = h.await?;
                last_height = height;
                match res {
                    Ok(block_val) => match parse_alchemy_header(&block_val) {
                        Ok(header) => {
                            headers_to_insert.push(header);
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse block {} response: {:?}", height, e);
                        }
                    },
                    Err(e) => {
                        tracing::error!("Failed to fetch block {}: {:?}", height, e);
                    }
                }
            }

            if !headers_to_insert.is_empty() {
                let count = headers_to_insert.len();
                let mut store_lock = store.lock().unwrap();
                if let Err(e) = store_lock.insert_headers_batch(&headers_to_insert) {
                    tracing::error!("Failed to insert batch of {} headers: {:?}", count, e);
                } else {
                    fetched += count;
                    let elapsed = start.elapsed().as_secs_f64();
                    let rps = fetched as f64 / elapsed;
                    tracing::info!(
                        "Progress: fetched={}/{} (height={}) rps={:.2}",
                        fetched,
                        total,
                        last_height,
                        rps
                    );
                }
            }
        }
    }

    // Drain remaining handles.
    let mut remaining_headers = Vec::new();
    let mut last_height = 0;
    for h in handles {
        let (height, res, _) = h.await?;
        last_height = height;
        match res {
            Ok(block_val) => match parse_alchemy_header(&block_val) {
                Ok(header) => {
                    remaining_headers.push(header);
                }
                Err(e) => {
                    tracing::error!("Failed to parse block {} response: {:?}", height, e);
                }
            },
            Err(e) => {
                tracing::error!("Failed to fetch block {}: {:?}", height, e);
            }
        }
    }

    if !remaining_headers.is_empty() {
        let count = remaining_headers.len();
        let mut store_lock = store.lock().unwrap();
        if let Err(e) = store_lock.insert_headers_batch(&remaining_headers) {
            tracing::error!("Failed to insert final batch of {} headers: {:?}", count, e);
        } else {
            fetched += count;
            let elapsed = start.elapsed().as_secs_f64();
            let rps = fetched as f64 / elapsed;
            tracing::info!(
                "Progress: fetched={}/{} (height={}) rps={:.2}",
                fetched,
                total,
                last_height,
                rps
            );
        }
    }

    tracing::info!(
        "Done. Fetched {}/{} blocks in {:.1}s  avg rps={:.2}",
        fetched,
        total,
        start.elapsed().as_secs_f64(),
        fetched as f64 / start.elapsed().as_secs_f64()
    );

    Ok(())
}
