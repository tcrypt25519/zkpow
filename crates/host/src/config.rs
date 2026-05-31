const DB_PATH_DEFAULT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../headers.db");

pub fn db_path() -> &'static str {
    match std::env::var("ZKPOW_DB_PATH") {
        Ok(path) => Box::leak(Box::new(path)),
        Err(_) => DB_PATH_DEFAULT,
    }
    //std::env::var("ZKPOW_DB_PATH").unwrap_(DB_PATH_DEFAULT)
}
