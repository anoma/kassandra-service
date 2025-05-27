//! Error type for the Kassandra client library

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Error in response from Kassandra service: {0}")]
    ServerError(String),
    #[error("{0}")]
    Io(std::io::Error),
    #[error("{0}")]
    MsgError(shared::MsgError),
    #[error("Establishing RA-TLS connection failed: {0}")]
    RATLS(String),
}
