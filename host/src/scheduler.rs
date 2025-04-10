//! Module for scheduling the events that the host should handle.

use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::future::{BoxFuture, FutureExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::Sleep;

use crate::db::InterruptFlag;

/// A struct for creating biased combined futures
/// for interrupts, incoming connections, and background work.
/// This will act as an event scheduler for the host
pub struct EventScheduler {
    listener: TcpListener,
    interrupt_flag: InterruptFlag,
}

impl EventScheduler {
    /// Create a new event scheduler
    pub fn new(listener: TcpListener, interrupt_flag: InterruptFlag) -> Self {
        Self {
            listener,
            interrupt_flag,
        }
    }

    /// Get the next scheduled event
    pub fn next_query(&mut self) -> NextQuery {
        NextQuery {
            accept: self.listener.accept().boxed(),
            dropped: self.interrupt_flag.dropped().boxed(),
            timeout: Box::pin(tokio::time::sleep(Duration::from_millis(10))),
        }
    }
}

/// The next scheduled event
pub enum NextEvent {
    /// An interrupt request was received
    Interrupt,
    /// A client request was received
    Accept(TcpStream),
    /// Updated registered keys against latest MASP txs.
    /// This is the default when incoming commands are not
    /// present.
    PerformFmd,
}

/// A future which first checks for an interrupt, then
/// checks for an incoming client, then defaults to performing
/// FMD. The default is spaced out with a small sleep to
/// prevent starving the other futures.
pub struct NextQuery<'f1, 'f2> {
    accept: BoxFuture<'f1, std::io::Result<(TcpStream, SocketAddr)>>,
    dropped: BoxFuture<'f2, bool>,
    timeout: Pin<Box<Sleep>>,
}

impl Future for NextQuery<'_, '_> {
    type Output = NextEvent;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.dropped.as_mut().poll(cx) {
            Poll::Ready(_) => Poll::Ready(NextEvent::Interrupt),
            Poll::Pending => match self.accept.as_mut().poll(cx) {
                Poll::Ready(Ok((stream, _))) => Poll::Ready(NextEvent::Accept(stream)),
                Poll::Ready(Err(e)) => {
                    tracing::error!(
                        "Encountered unexpected error while listening for new connections: {e}"
                    );
                    Poll::Ready(NextEvent::PerformFmd)
                }
                _ => match self.timeout.as_mut().poll(cx) {
                    Poll::Ready(_) => Poll::Ready(NextEvent::PerformFmd),
                    Poll::Pending => Poll::Pending,
                },
            },
        }
    }
}
