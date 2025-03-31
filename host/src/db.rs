//! Implementation of the backing DB of the service.

mod fetch;
mod utils;

use eyre::WrapErr;
use rusqlite::Connection;
pub use utils::DropFlag;

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
    updating: Option<std::thread::JoinHandle<Result<(), eyre::Error>>>,
}

impl DB {
    /// Create new connections to the dbs. Creates directories/files and initializes
    /// tables if necessary.
    pub fn new() -> eyre::Result<Self> {
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
        let fmd = if !fmd_db_path.exists() {
            let fmd = Connection::open(fmd_db_path).wrap_err("Failed to open the FMD DB")?;
            fmd.execute(
                "CREATE TABLE Indices (
                id INTEGER PRIMARY KEY,
                owner BLOB NOT NULL,
                idx_set BLOC NOT NULL
            )",
                (),
            )
            .wrap_err("Failed to creat FMD DB table")?;
            fmd
        } else {
            Connection::open(fmd_db_path).wrap_err("Failed to creat FMD DB table")?
        };

        Ok(Self {
            masp,
            fmd,
            updating: None,
        })
    }

    /// Spawn the update job in the background and save a handle to it.
    pub fn start_updates(
        &mut self,
        url: reqwest::Url,
        max_wal_size: usize,
        interrupt: DropFlag,
    ) -> eyre::Result<()> {
        let masp_db_path = kassandra_dir().join(MASP_DB_PATH);
        let conn = Connection::open(masp_db_path).wrap_err("Failed to creat MASP DB table")?;
        let mut fetcher = Fetcher::new(url, conn, max_wal_size)?;
        let handle = std::thread::spawn(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move { fetcher.run().await })?;
            drop(interrupt);
            Ok(())
        });
        self.updating = Some(handle);
        Ok(())
    }

    pub fn close(mut self) {
        tracing::info!(
            "Closing the DB and stopping the update job. Please be patient, this can take some time..."
        );
        _ = self.masp.close();
        _ = self.fmd.close();
        if let Some(update) = self.updating.take() {
            _ = update.join().unwrap();
        }
    }
}
