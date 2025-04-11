use std::ops::ControlFlow;
use std::time::Duration;

use borsh::BorshDeserialize;
use eyre::Context;
use futures::future::{Either, select};
use futures::stream::{FuturesUnordered, StreamExt};
use namada::borsh::BorshSerializeExt;
use namada::chain::BlockHeight;
use namada::control_flow::{ShutdownSignal, ShutdownSignalChan, install_shutdown_signal};
use namada::masp::IndexerMaspClient;
use namada::masp::utils::{IndexedNoteData, IndexedNoteEntry, MaspClient};
use rusqlite::Connection;
use tokio::task::JoinHandle;

use crate::config::kassandra_dir;
use crate::db::utils::{AsyncCounter, AtomicFlag, FetchedRanges, TaskError};

const BATCH_SIZE: usize = 30;
const DEFAULT_BUF_SIZE: usize = 32;

const FETCHER_FILE: &str = "fetcher.dat";
pub type Fetched =
    Result<(BlockHeight, BlockHeight, Vec<IndexedNoteEntry>), TaskError<[BlockHeight; 2]>>;

/// The tasks fetching data from a MASP indexer
#[derive(Clone)]
struct Tasks {
    message_receiver: flume::Receiver<Fetched>,
    message_sender: flume::Sender<Fetched>,
    /// A thread-safe counter of the number of tasks running
    active_tasks: AsyncCounter,
}

macro_rules! db_error {
   ($($arg:tt)*) => {{
       if *$crate::LOG_FETCH_ERRORS.get().unwrap() {
            tracing::error!("{}", format_args_nl!($($arg)*));
       }
    }};
}

impl Tasks {
    async fn get_next_message(&mut self, interrupt: AtomicFlag) -> Option<Fetched> {
        if interrupt.get() {
            return None;
        }
        if let Either::Left((maybe_message, _)) =
            select(self.message_receiver.recv_async(), &mut self.active_tasks).await
        {
            let Ok(message) = maybe_message else {
                unreachable!("There must be at least one sender alive");
            };
            Some(message)
        } else {
            // NB: queueing a message to a channel doesn't mean we
            // actually consume it. we must wait for the channel to
            // be drained when all tasks have returned. the spin loop
            // hint below helps the compiler to optimize the `try_recv`
            // branch, to avoid maxing out the cpu.
            std::hint::spin_loop();
            self.message_receiver.try_recv().ok()
        }
    }
}

#[derive(Debug, PartialEq)]
enum FetcherState {
    Normal,
    Interrupted,
}

/// A buffered DB connection
struct DbConn {
    conn: Connection,
    wal: IndexedNoteData,
    max_wal_size: usize,
}

impl DbConn {
    fn extend<I>(&mut self, items: I)
    where
        I: IntoIterator<Item = IndexedNoteEntry>,
    {
        self.wal.extend(items);
        if self.wal.len() >= self.max_wal_size {
            tracing::info!("WAL limit reached, flushing to DB");
            let wal = std::mem::take(&mut self.wal);
            let mut stmt = self
                .conn
                .prepare("INSERT INTO Txs (idx, height, data, flag) VALUES (?1, ?2, ?3, ?4)")
                .unwrap();
            for (idx, tx) in wal {
                // TODO: Add fmd flag
                stmt.execute((
                    idx.serialize_to_vec(),
                    idx.block_height.0,
                    tx.serialize_to_vec(),
                    "",
                ))
                .unwrap();
            }
        }
    }
}

impl Drop for DbConn {
    fn drop(&mut self) {
        let wal = std::mem::take(&mut self.wal);
        let Ok(mut stmt) = self
            .conn
            .prepare("INSERT INTO Txs (idx, height, data, flag) VALUES (?1, ?2, ?3, ?4)")
        else {
            return;
        };
        for (idx, tx) in wal {
            // TODO: Add fmd flag
            _ = stmt
                .execute((
                    idx.serialize_to_vec(),
                    idx.block_height.0,
                    tx.serialize_to_vec(),
                    "",
                ))
                .unwrap();
        }
    }
}

/// The type in charge of keeping the DB in sync with a MASP indexer
/// by downloading the latest MASP txs.
pub struct Fetcher {
    /// The block we are synced up to
    fetched: FetchedRanges,
    /// A client for talking with a MASP indexer
    indexer: IndexerMaspClient,
    /// A db connection
    conn: DbConn,
    /// A set of active fetching tasks
    tasks: Tasks,
    /// A listener for a shutdown signal that is used for graceful shutdowns.
    interrupt_flag: AtomicFlag,
    /// Keeps track of interrupts and errors while fetching
    state: FetcherState,
    /// Listens for interrupt signals
    shutdown_signal: ShutdownSignalChan,
    /// A channel to communicate the block height synced to completely
    synced_to: tokio::sync::watch::Sender<u64>,
}

impl Fetcher {
    /// Create a new fetcher
    pub fn new(
        url: reqwest::Url,
        conn: Connection,
        synced_to: tokio::sync::watch::Sender<u64>,
        max_wal_size: usize,
    ) -> eyre::Result<Self> {
        let indexer_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(60))
            .build()
            .unwrap();

        let (message_sender, message_receiver) = flume::bounded(DEFAULT_BUF_SIZE);
        let shutdown_signal = install_shutdown_signal(true);
        let fetched_ranges = if let Ok(bytes) = std::fs::read(kassandra_dir().join(FETCHER_FILE)) {
            <FetchedRanges as BorshDeserialize>::try_from_slice(&bytes).wrap_err(format!(
                "Failed to deserialize the contents of {FETCHER_FILE}"
            ))?
        } else {
            FetchedRanges::default()
        };

        Ok(Self {
            fetched: fetched_ranges,
            indexer: IndexerMaspClient::new(indexer_client, url, true, 100),
            conn: DbConn {
                conn,
                wal: Default::default(),
                max_wal_size,
            },
            tasks: Tasks {
                message_receiver,
                message_sender,
                active_tasks: AsyncCounter::new(),
            },
            interrupt_flag: Default::default(),
            state: FetcherState::Normal,
            shutdown_signal,
            synced_to,
        })
    }

    /// Runs a loop until interrupted. Each iteration of the loop
    /// queries the latest block height and downloads batches of
    /// MASP txs up to that block height and saves them to the DB.
    pub async fn run(&mut self) -> Result<(), eyre::Error> {
        // keep trying to sync to the tip of the chain
        loop {
            self.check_exit_conditions();
            if self.state == FetcherState::Interrupted {
                return Ok(());
            }
            match self.sync().await? {
                ControlFlow::Break(_) => return Ok(()),
                ControlFlow::Continue(_) => {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            }
            core::hint::spin_loop();
        }
    }

    /// Fetch all masp txs up to the tip of the chain
    async fn sync(&mut self) -> Result<ControlFlow<()>, eyre::Error> {
        let Ok(Some(latest_height)) = self.indexer.last_block_height().await else {
            tracing::error!(
                "Could not fetch latest block from MASP Indexer, check to provided URL."
            );
            return Err(eyre::eyre!(
                "Could not fetch latest block from MASP Indexer."
            ));
        };
        let synced_to = self.fetched.first();
        if synced_to > latest_height {
            tracing::info!("Synced.");
            return Ok(ControlFlow::Continue(()));
        }

        tracing::info!(
            "Fetching from block {}..={}",
            self.fetched.first(),
            latest_height
        );

        // spawn fetch jobs and add them to a `FuturesUnordered`
        let handles = FuturesUnordered::new();

        for from in (self.fetched.first().0..=latest_height.0).step_by(BATCH_SIZE) {
            let to = (from + BATCH_SIZE as u64 - 1).min(latest_height.0);
            for [from, to] in self.fetched.blocks_left_to_fetch(from, to) {
                let handle = tokio::task::spawn(Fetcher::spawn_fetch_txs(
                    self.indexer.clone(),
                    self.tasks.clone(),
                    self.shutdown_signal.clone(),
                    self.interrupt_flag.clone(),
                    from,
                    to,
                ));
                handles.push(handle);
            }
        }
        // handle messages generated by the spawned jobs
        while let Some(fetched) = self
            .tasks
            .get_next_message(self.interrupt_flag.clone())
            .await
        {
            self.check_exit_conditions();
            if let Some(handle) = self.handle_fetched(fetched) {
                handles.push(handle);
            }
        }
        self.check_exit_conditions();

        // check if the process has received a shutdown signal
        match std::mem::replace(&mut self.state, FetcherState::Normal) {
            FetcherState::Interrupted => Ok(ControlFlow::Break(())),
            FetcherState::Normal => {
                for result in <FuturesUnordered<_> as StreamExt>::collect::<Vec<_>>(handles).await {
                    if let Err(e) = result {
                        tracing::error!("Fetch task panicked with {e}");
                    }
                }
                Ok(ControlFlow::Continue(()))
            }
        }
    }

    /// If blocks fetched successfully, write to db. Otherwise, retry fetching
    /// them.
    fn handle_fetched(&mut self, fetched: Fetched) -> Option<JoinHandle<()>> {
        match fetched {
            Ok((from, to, fetched)) => {
                self.fetched.insert(from, to);
                // update the block height we are completely synced up to
                // N.B. this subtraction is safe
                _ = self.synced_to.send(self.fetched.first().0 - 1);
                self.conn.extend(fetched);
                None
            }
            Err(TaskError {
                error,
                context: [from, to],
            }) => {
                db_error!("Fetch task encountered error: {error}");
                if !matches!(self.state, FetcherState::Interrupted) {
                    Some(tokio::task::spawn(Fetcher::spawn_fetch_txs(
                        self.indexer.clone(),
                        self.tasks.clone(),
                        self.shutdown_signal.clone(),
                        self.interrupt_flag.clone(),
                        from,
                        to,
                    )))
                } else {
                    None
                }
            }
        }
    }

    /// Spawn a new fetch task
    async fn spawn_fetch_txs(
        client: IndexerMaspClient,
        tasks: Tasks,
        mut shutdown: ShutdownSignalChan,
        interrupt_flag: AtomicFlag,
        from: BlockHeight,
        to: BlockHeight,
    ) {
        let fetch = async {
            let ret = client.fetch_shielded_transfers(from, to).await;
            if let Err(e) = &ret {
                db_error!("Fetching encountered error {e}");
            }
            let ret = ret.wrap_err("Failed to fetch shielded transfers");
            ret.map_err(|error| TaskError {
                error,
                context: [from, to],
            })
            .map(|fetched| (from, to, fetched))
        };
        tokio::select! {
            msg = fetch => {
                tasks.message_sender.send_async(msg).await.unwrap();
            }
            _ = shutdown.wait_for_shutdown() => {
                interrupt_flag.set();
            }
        }
    }

    fn check_exit_conditions(&mut self) {
        if matches!(&self.state, FetcherState::Interrupted) {
            return;
        }
        if self.shutdown_signal.received() {
            self.interrupt_flag.set();
            self.state = FetcherState::Interrupted;
        }
    }

    /// Save the blocks fetched to file.
    pub(crate) fn save(&self) {
        let to_save = self.fetched.serialize_to_vec();
        std::fs::write(kassandra_dir().join(FETCHER_FILE), to_save).unwrap();
    }
}
