//! Implementation of the backing DB of the service.

mod fetch;
mod utils;

use std::str::FromStr;

use borsh::BorshDeserialize;
use eyre::WrapErr;
use fmd::fmd2_compact::FlagCiphertexts;
use namada::tx::IndexedTx;
use rusqlite::Connection;
use shared::db::{EncryptedResponse, Index};
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
    /// A channel the fetch job uses to communicate to which block height
    /// we are completely synced.
    synced_to: Option<tokio::sync::watch::Receiver<u64>>,
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
                height INTEGER NOT NULL,
                data BLOB NOT NULL,
                flag TEXT
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
                owner TEXT NOT NULL PRIMARY KEY,
                nonce BLOB NOT NULL,
                idx_set BLOB NOT NULL,
                height: INTEGER NOT NULL
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
                synced_to: None,
            },
            uuid,
        ))
    }

    /// Get all flags of MASP txs at the requested block height
    pub fn get_height(
        &mut self,
        height: u64,
    ) -> eyre::Result<Vec<(Index, Option<FlagCiphertexts>)>> {
        let mut stmt = self
            .masp
            .prepare("SELECT idx, flag FROM Txs WHERE height=?1")
            .unwrap();
        let rows: Vec<Result<(Vec<u8>, String), _>> = stmt
            .query_map([height], |row| Ok((row.get(0)?, row.get(1)?)))
            .wrap_err("Database query failed")?
            .collect();
        Ok(rows
            .into_iter()
            .map(|res| match res {
                Ok((idx, flag_str)) => {
                    let Ok(idx) = <IndexedTx as BorshDeserialize>::try_from_slice(&idx)
                        .map(|ix|  Index{ height: ix.block_height.0, tx: ix.block_index.0 })else {
                        panic!("Could not deserialize `IndexedTx` of masp tx at height: {height}");
                    };
                    let flag = serde_json::from_str::<FlagCiphertexts>(&flag_str)
                        .map(Some)
                        .unwrap_or_else(|e| {
                            tracing::debug!(
                                "Could not deserialize `FlagCiphertext` of a row at height {height}: {e}"
                            );
                            None
                        });
                    (idx, flag)
                }
                Err(err) => {
                    panic!("Failed to read masp txs at height {height} from DB: {err}");
                }
            })
            .collect())
    }

    /// Update the DB with the latest encrypted index set per user
    pub fn update_indices(&mut self, new_indices: Vec<EncryptedResponse>) -> eyre::Result<()> {
        let mut stmt = self
            .fmd
            .prepare("INSERT OR REPLACE INTO Indices(nonce, idx_set, owner, height) VALUES (?1, ?2, ?3, ?4)")
            .unwrap();
        for EncryptedResponse {
            owner,
            nonce,
            indices,
            height,
        } in new_indices
        {
            stmt.execute((nonce, indices, owner, height))
                .wrap_err("Could not update FMD db")?;
        }
        Ok(())
    }

    /// Get the encrypted index set belonging to a registered key
    pub fn fetch_indices(&self, user: &str) -> eyre::Result<EncryptedResponse> {
        let (owner, n, indices, height) = self
            .fmd
            .query_row::<(String, Vec<u8>, Vec<u8>, u64), _, _>(
                "SELECT owner, nonce, idx_set, height FROM Indices WHERE owner=?1",
                rusqlite::params![user],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .wrap_err("Could not find user's key hash in the DB")?;
        Ok(EncryptedResponse {
            owner,
            nonce: n.try_into().unwrap(),
            indices,
            height,
        })
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
        let (send, recv) = tokio::sync::watch::channel(1u64);
        let mut fetcher = Fetcher::new(url, conn, send, max_wal_size)?;
        let handle = tokio::task::spawn(async move {
            let ret = fetcher.run().await;
            fetcher.save();
            drop(interrupt);
            ret
        });
        self.updating = Some(handle);
        self.synced_to = Some(recv);
        Ok(())
    }

    /// Get the block height we are synced up to compeletly
    pub fn synced_to(&self) -> u64 {
        let Some(recv) = &self.synced_to else {
            return 1;
        };
        *recv.borrow()
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
