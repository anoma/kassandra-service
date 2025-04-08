//! Implementation of the backing DB of the service.

mod fetch;
mod utils;

use eyre::WrapErr;
use rusqlite::Connection;
use std::str::FromStr;
pub use utils::InterruptFlag;
use uuid::Uuid;

use crate::config::kassandra_dir;
use crate::db::fetch::Fetcher;

const MASP_DB_PATH: &str = "masp.db3";
const FMD_DB_PATH: &str = "fmd.db3";

/// The backing database implementation
pub struct DB {
    /// Connection to the DB holding MASP txs
    masp: Connection,
    /// Connection to the DB holding the index sets for registered keys
    fmd: Connection,
    /// A handle to the job updating the MASP DB
    updating: Option<tokio::task::JoinHandle<Result<(), eyre::Error>>>,
}

impl DB {
    /// Create new connections to the DBs. Creates directories/files and initializes
    /// tables and UUID if necessary. Returns a handle to the DBs and the created /
    /// read UUID.
    pub fn new() -> eyre::Result<(Self, Uuid)> {
        let masp_db_path = kassandra_dir().join(MASP_DB_PATH);
        let masp = if !masp_db_path.exists() {
            let masp = Connection::open(masp_db_path).wrap_err("Failed to open the MAPS DB")?;
            masp.execute(
                "CREATE TABLE Txs (
                id INTEGER PRIMARY KEY,
                idx BLOB NOT NULL,
                data BLOB NOT NULL,
                flag BLOB
                )",
                (),
            )
            .wrap_err("Failed to create MASP DB table")?;
            masp
        } else {
            Connection::open(masp_db_path).wrap_err("Failed to open the MAPS DB")?
        };

        let fmd_db_path = kassandra_dir().join(FMD_DB_PATH);
        let (fmd, uuid) = if !fmd_db_path.exists() {
            let fmd = Connection::open(fmd_db_path).wrap_err("Failed to open the FMD DB")?;
            fmd.execute(
                "CREATE TABLE Indices (
                id INTEGER PRIMARY KEY,
                owner BLOB NOT NULL,
                idx_set BLOB NOT NULL
            )",
                (),
            )
            .wrap_err("Failed to creat FMD DB table")?;
            // create and persist a UUID
            fmd.execute(
                "CREATE TABLE UUID (
                id INTEGER PRIMARY KEY,
                uuid TEXT NOT NULL
                )",
                (),
            )
            .wrap_err("Failed to creat FMD DB table")?;
            let uuid = Uuid::new_v4();
            fmd.execute("INSERT INTO UUID (uuid) VALUES (?1)", (&uuid.to_string(),))
                .wrap_err("Could not insert UUID into DB")?;
            (fmd, uuid)
        } else {
            let fmd = Connection::open(fmd_db_path).wrap_err("Failed to creat FMD DB table")?;
            let uuid = fmd
                .query_row::<String, _, _>("SELECT uuid FROM UUID LIMIT 1", [], |row| row.get(0))
                .wrap_err("Could not  retrieve UUID from DB")?;
            let uuid = Uuid::from_str(&uuid).wrap_err("Could not parse UUID from DB")?;
            (fmd, uuid)
        };

        Ok((
            Self {
                masp,
                fmd,
                updating: None,
            },
            uuid,
        ))
    }

    /// Spawn the update job in the background and save a handle to it.
    pub fn start_updates(
        &mut self,
        url: reqwest::Url,
        max_wal_size: usize,
        interrupt: InterruptFlag,
    ) -> eyre::Result<()> {
        let masp_db_path = kassandra_dir().join(MASP_DB_PATH);
        let conn = Connection::open(masp_db_path).wrap_err("Failed to creat MASP DB table")?;
        let mut fetcher = Fetcher::new(url, conn, max_wal_size)?;
        let handle = tokio::task::spawn(async move {
            let ret = fetcher.run().await;
            fetcher.save();
            drop(interrupt);
            ret
        });
        self.updating = Some(handle);
        Ok(())
    }

    pub async fn close(mut self) {
        tracing::info!("Closing the DB and stopping the update job...");
        _ = self.masp.close();
        _ = self.fmd.close();
        if let Some(update) = self.updating.take() {
            _ = update.await;
        }
    }
}
