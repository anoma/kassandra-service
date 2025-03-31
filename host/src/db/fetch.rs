use std::ops::ControlFlow;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

use borsh::BorshDeserialize;
use eyre::{Context, eyre};
use futures::future::{Either, select};
use namada::borsh::BorshSerializeExt;
use namada::chain::BlockHeight;
use namada::control_flow::{ShutdownSignal, ShutdownSignalChan, install_shutdown_signal};
use namada::hints;
use namada::masp::IndexerMaspClient;
use namada::masp::utils::{IndexedNoteData, IndexedNoteEntry, MaspClient};
use rusqlite::Connection;

use crate::config::kassandra_dir;
use crate::db::utils::{AsyncCounter, AtomicFlag, FetchedRanges, PanicFlag, TaskError};

const BATCH_SIZE: usize = 30;
const DEFAULT_BUF_SIZE: usize = 32;

const FETCHER_FILE: &str = "fetcher.dat";
pub type Fetched =
    Result<(BlockHeight, BlockHeight, Vec<IndexedNoteEntry>), TaskError<[BlockHeight; 2]>>;

/// The tasks fetching data from a MASP indexer
struct Tasks {
    message_receiver: flume::Receiver<Fetched>,
    message_sender: flume::Sender<Fetched>,
    /// A thread-safe counter of the number of tasks running
    active_tasks: AsyncCounter,
    /// A thread-safe flag indicating a panic happened in a task
    panic_flag: PanicFlag,
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

#[derive(Debug)]
enum FetcherState {
    Normal,
    Interrupted,
    Errored(eyre::Error),
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
            let mut stmt = self.conn.prepare("INSERT INTO Txs (?1, ?2, ?3)").unwrap();
            for (idx, tx) in wal {
                // TODO: Add fmd flag
                stmt.execute([idx.serialize_to_vec(), tx.serialize_to_vec(), vec![]])
                    .unwrap();
            }
        }
    }
}

impl Drop for DbConn {
    fn drop(&mut self) {
        let wal = std::mem::take(&mut self.wal);
        let Ok(mut stmt) = self.conn.prepare("INSERT INTO Txs (?1, ?2, ?3)") else {
            return;
        };
        for (idx, tx) in wal {
            // TODO: Add fmd flag
            _ = stmt
                .execute([idx.serialize_to_vec(), tx.serialize_to_vec(), vec![]])
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
}

impl Fetcher {
    /// Create a new fetcher
    pub fn new(url: reqwest::Url, conn: Connection, max_wal_size: usize) -> eyre::Result<Self> {
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
            indexer: IndexerMaspClient::new(indexer_client, url, true, 50),
            conn: DbConn {
                conn,
                wal: Default::default(),
                max_wal_size,
            },
            tasks: Tasks {
                message_receiver,
                message_sender,
                active_tasks: AsyncCounter::new(),
                panic_flag: PanicFlag::default(),
            },
            interrupt_flag: Default::default(),
            state: FetcherState::Normal,
            shutdown_signal,
        })
    }

    /// Runs a loop until interrupted. Each iteration of the loop
    /// queries the latest block height and downloads batches of
    /// MASP txs up to that block height and saves them to the DB.
    pub async fn run(&mut self) -> Result<(), eyre::Error> {
        self.check_exit_conditions();
        // keep trying to sync to the tip of the chain
        loop {
            if let ControlFlow::Break(_) = self.sync().await? {
                return Ok(());
            }
        }
    }

    /// Fetch all masp txs up to the tip of the chain
    async fn sync(&mut self) -> Result<ControlFlow<()>, eyre::Error> {
        let Ok(Some(latest_height)) = self.indexer.last_block_height().await else {
            return Err(eyre::eyre!(
                "Could not fetch latest block from MASP Indexer."
            ));
        };
        tracing::info!(
            "Fetching from block {}..={}",
            self.fetched.first(),
            latest_height
        );
        for from in (self.fetched.first().0..=latest_height.0).step_by(BATCH_SIZE) {
            let to = (from + BATCH_SIZE as u64 - 1).min(latest_height.0);
            for [from, to] in self.fetched.blocks_left_to_fetch(from, to) {
                self.spawn_fetch_txs(from, to)
            }
        }
        while let Some(fetched) = self
            .tasks
            .get_next_message(self.interrupt_flag.clone())
            .await
        {
            self.check_exit_conditions();
            self.handle_fetched(fetched);
        }
        match std::mem::replace(&mut self.state, FetcherState::Normal) {
            FetcherState::Errored(err) => Err(err),
            FetcherState::Interrupted => Ok(ControlFlow::Break(())),
            FetcherState::Normal => Ok(ControlFlow::Continue(())),
        }
    }

    fn handle_fetched(&mut self, fetched: Fetched) {
        match fetched {
            Ok((from, to, fetched)) => {
                tracing::info!("Fetched blocks {from}..={to} ");
                self.fetched.insert(from, to);
                self.save();
                self.conn.extend(fetched);
            }
            Err(TaskError {
                error,
                context: [from, to],
            }) => {
                tracing::error!("Fetch task encountered error: {error}");
                if !matches!(
                    self.state,
                    FetcherState::Errored(_) | FetcherState::Interrupted
                ) {
                    self.spawn_fetch_txs(from, to)
                }
            }
        }
    }

    /// Spawn a new fetch task
    fn spawn_fetch_txs(&self, from: BlockHeight, to: BlockHeight) {
        let sender = self.tasks.message_sender.clone();
        let guard = (
            self.tasks.active_tasks.clone(),
            self.tasks.panic_flag.clone(),
        );
        let client = self.indexer.clone();
        let interrupt = self.interrupt_flag.clone();
        tokio::task::spawn(async move {
            let _guard = guard;
            let wrapped_fut = std::future::poll_fn(move |cx| {
                if interrupt.get() {
                    Poll::Ready(None)
                } else {
                    Pin::new(&mut Box::pin({
                        let client = client.clone();
                        async move {
                            client
                                .fetch_shielded_transfers(from, to)
                                .await
                                .wrap_err("Failed to fetch shielded transfers")
                                .map_err(|error| TaskError {
                                    error,
                                    context: [from, to],
                                })
                                .map(|fetched| (from, to, fetched))
                        }
                    }))
                    .poll(cx)
                    .map(Some)
                }
            });
            if let Some(msg) = wrapped_fut.await {
                sender.send_async(msg).await.unwrap()
            }
        });
    }

    fn check_exit_conditions(&mut self) {
        if hints::unlikely(self.tasks.panic_flag.panicked()) {
            self.state = FetcherState::Errored(eyre!(
                "A worker thread panicked during the shielded sync".to_string(),
            ));
        }
        if matches!(
            &self.state,
            FetcherState::Interrupted | FetcherState::Errored(_)
        ) {
            return;
        }
        if self.shutdown_signal.received() {
            self.state = FetcherState::Interrupted;
            self.interrupt_flag.set();
        }
    }

    fn save(&self) {
        let to_save = self.fetched.serialize_to_vec();
        std::fs::write(kassandra_dir().join(FETCHER_FILE), to_save).unwrap();
    }
}
