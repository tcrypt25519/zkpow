use super::{
    hash_header, target_from_bits, work_from_target, BlockHash, BlockTimestamp, ChainWork,
    CompactTarget, Header, NewHeader, State, Target, GENESIS_TARGET,
};

#[derive(Debug, Clone)]
pub struct HeaderRecord {
    pub height: u64,
    pub header: Header,
    pub chain_work: ChainWork,
    pub median_time_past: BlockTimestamp,
}

#[derive(Debug, Clone)]
pub struct HeaderBatchWitness {
    pub headers: Vec<NewHeader>,
    pub median_time_past_hints: Vec<BlockTimestamp>,
}

pub fn load_header_records_from_db(
    db_path: &str,
    start_height: u64,
    count: u64,
) -> Vec<HeaderRecord> {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open SQLite database");

    let mut stmt = conn
        .prepare(
            "SELECT height, version, prev, merkle_root, timestamp, n_bits, nonce, chainwork, median_time_past FROM headers WHERE height >= ?1 AND height < ?2 ORDER BY height ASC",
        )
        .unwrap_or_else(|err| {
            panic!("failed to prepare SQL statement for db {}: {}", db_path, err)
        });

    let end_height = start_height + count;
    let rows = stmt
        .query_map(rusqlite::params![start_height, end_height], |row| {
            let height: i64 = row.get(0)?;
            let version: i64 = row.get(1)?;
            let prev: Vec<u8> = row.get(2)?;
            let merkle_root: Vec<u8> = row.get(3)?;
            let timestamp: i64 = row.get(4)?;
            let nbits: i64 = row.get(5)?;
            let nonce: i64 = row.get(6)?;
            let chainwork: Vec<u8> = row.get(7)?;
            let median_time_past: i64 = row.get(8)?;

            let header = Header {
                version: version as u32,
                prev_blockhash: BlockHash::new(prev.try_into().expect("prev must be 32 bytes")),
                merkle_root: merkle_root
                    .try_into()
                    .expect("merkle_root must be 32 bytes"),
                timestamp: BlockTimestamp::new(timestamp as u32),
                compact_target: CompactTarget::new(nbits as u32),
                nonce: nonce as u32,
            };

            Ok(HeaderRecord {
                height: height as u64,
                header,
                chain_work: chain_work_from_db_bytes(&chainwork),
                median_time_past: BlockTimestamp::new(median_time_past as u32),
            })
        })
        .expect("failed to execute query");

    let mut records = Vec::with_capacity(count as usize);
    for row_result in rows {
        records.push(row_result.expect("failed to read header record from database"));
    }

    records
}

fn chain_work_from_db_bytes(bytes: &[u8]) -> ChainWork {
    let raw: [u8; 32] = bytes.try_into().expect("chainwork must be 32 bytes");
    let mut little_endian = raw;
    little_endian.reverse();
    ChainWork::from_le_bytes(little_endian)
}

pub fn load_header_record_from_db(db_path: &str, height: u64) -> HeaderRecord {
    load_header_records_from_db(db_path, height, 1)
        .into_iter()
        .next()
        .expect("exactly one header record should be returned")
}

pub fn load_header_batch_witness_from_db(
    db_path: &str,
    start_height: u64,
    count: u64,
) -> HeaderBatchWitness {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open SQLite database");

    let mut stmt = conn
        .prepare(
            "SELECT version, merkle_root, timestamp, nonce, median_time_past FROM headers WHERE height >= ?1 AND height < ?2 ORDER BY height ASC",
        )
        .unwrap_or_else(|err| {
            panic!("failed to prepare batch witness SQL statement for db {}: {}", db_path, err)
        });

    let end_height = start_height + count;
    let rows = stmt
        .query_map(rusqlite::params![start_height, end_height], |row| {
            let version: i64 = row.get(0)?;
            let merkle_root: Vec<u8> = row.get(1)?;
            let timestamp: i64 = row.get(2)?;
            let nonce: i64 = row.get(3)?;
            let median_time_past: i64 = row.get(4)?;

            Ok((
                NewHeader {
                    version: version as u32,
                    merkle_root: merkle_root
                        .try_into()
                        .expect("merkle_root must be 32 bytes"),
                    timestamp: BlockTimestamp::new(timestamp as u32),
                    nonce: nonce as u32,
                },
                BlockTimestamp::new(median_time_past as u32),
            ))
        })
        .expect("failed to execute batch witness query");

    let mut headers = Vec::with_capacity(count as usize);
    let mut median_time_past_hints = Vec::with_capacity(count as usize);
    for row_result in rows {
        let (header, median_time_past) =
            row_result.expect("failed to read header witness row from database");
        headers.push(header);
        median_time_past_hints.push(median_time_past);
    }

    HeaderBatchWitness {
        headers,
        median_time_past_hints,
    }
}

pub fn load_new_headers_from_db(db_path: &str, start_height: u64, count: u64) -> Vec<NewHeader> {
    load_header_batch_witness_from_db(db_path, start_height, count).headers
}

pub fn genesis_state_from_record(genesis: HeaderRecord, genesis_hash: BlockHash) -> State {
    let block_hash = hash_header(&genesis.header);
    assert_eq!(
        block_hash, genesis_hash,
        "configured genesis hash must match the supplied genesis header",
    );
    let genesis_work = work_from_target(GENESIS_TARGET).expect("GENESIS_TARGET is a valid target");

    let mut timestamps = [BlockTimestamp::default(); zkpow_core::WINDOW_SIZE];
    timestamps[0] = genesis.header.timestamp;

    State {
        header: genesis.header,
        block_hash,
        genesis_hash,
        current_nbits: genesis.header.compact_target,
        height: genesis.height as u32,
        chain_work: genesis.chain_work,
        current_work: genesis_work,
        current_target: GENESIS_TARGET,
        epoch_start_timestamp: genesis.header.timestamp,
        timestamps,
    }
}

pub fn state_from_db_at_height(db_path: &str, height: u32, genesis_hash: BlockHash) -> State {
    if height == 0 {
        let genesis = load_header_record_from_db(db_path, 0);
        return genesis_state_from_record(genesis, genesis_hash);
    }

    let current = load_header_record_from_db(db_path, height as u64);
    let epoch_start_height = (height / zkpow_core::EPOCH_LENGTH) * zkpow_core::EPOCH_LENGTH;
    let epoch_start_record = load_header_record_from_db(db_path, epoch_start_height as u64);
    let window_count = (height as usize + 1).min(zkpow_core::WINDOW_SIZE) as u64;
    let window_start = height as u64 + 1 - window_count;
    let window_records = load_header_records_from_db(db_path, window_start, window_count);

    let mut timestamps = [BlockTimestamp::default(); zkpow_core::WINDOW_SIZE];
    for record in window_records {
        timestamps[record.height as usize % zkpow_core::WINDOW_SIZE] = record.header.timestamp;
    }

    let current_target: Target = target_from_bits(current.header.compact_target);

    State {
        header: current.header,
        block_hash: hash_header(&current.header),
        genesis_hash,
        current_nbits: current.header.compact_target,
        height,
        chain_work: current.chain_work,
        current_work: work_from_target(current_target).expect("DB target must be a valid target"),
        current_target,
        epoch_start_timestamp: epoch_start_record.header.timestamp,
        timestamps,
    }
}
