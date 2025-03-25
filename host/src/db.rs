//! Implementation of the backing DB of the service.
use std::thread::JoinHandle;

use rusqlite::Connection;

use crate::config::kassandra_dir;

const MASP_DB_PATH: &str = "masp.db3";
const FMD_DB_PATH: &str = "fmd.db3";

pub struct DB {
    masp: Connection,
    fmd: Connection,
}

impl DB {
    /// Create new connections to the dbs. Creates directories/files and initializes
    /// tables if necessary.
    pub fn new() -> rusqlite::Result<Self> {
        let masp_db_path = kassandra_dir().join(MASP_DB_PATH);
        let masp = if !masp_db_path.exists() {
            let masp = Connection::open(masp_db_path)?;
            masp.execute(
                "CREATE TABLE txs (
                id INTEGER PRIMARY KEY,
                idx INTEGER NOT NULL,
                data BLOB NOT NULL,
                flag BLOB NOT NULL
                )",
                (),
            )?;
            masp
        } else {
            Connection::open(masp_db_path)?
        };

        let fmd_db_path = kassandra_dir().join(FMD_DB_PATH);
        let fmd = if !fmd_db_path.exists() {
            let fmd = Connection::open(fmd_db_path)?;
            fmd.execute(
                "CREATE TABLE indices (
                id INTEGER PRIMARY KEY,
                owner BLOB NOT NULL,
                idx_set BLOC NOT NULL
            )",
                (),
            )?;
            fmd
        } else {
            Connection::open(fmd_db_path)?
        };

        Ok(Self { masp, fmd })
    }
}
