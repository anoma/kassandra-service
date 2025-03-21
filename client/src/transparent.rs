use shared::tee::EnclaveClient;
use thiserror::Error;

use crate::transparent::VerifyError::UserData;

#[derive(Error, Debug)]
pub enum VerifyError {
    #[error("User data was not 64 bytes")]
    UserData,
}
#[derive(Copy, Clone)]
pub struct TClient;

impl EnclaveClient for TClient {
    type Error = VerifyError;

    fn verify_quote(report: &[u8], _: u64) -> Result<[u8; 64], Self::Error> {
        report.try_into().map_err(|_| UserData)
    }
}
