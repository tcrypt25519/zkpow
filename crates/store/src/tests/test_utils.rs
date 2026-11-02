use crate::{db::SQLITE_MEMORY_DATABASE, HeaderStore};

const TEST_FIXTURE_HEADERS_SQL: &str = include_str!("test_fixture_headers.sql");

pub fn in_memory_store() -> HeaderStore {
    HeaderStore::open(SQLITE_MEMORY_DATABASE).expect("failed to open in-memory store")
}

pub fn create_test_fixture_store() -> HeaderStore {
    let store = in_memory_store();
    store
        .conn
        .execute_batch(TEST_FIXTURE_HEADERS_SQL)
        .expect("failed to insert test fixture headers");
    store
}
