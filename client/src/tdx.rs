//! TDX specific implementation for verifying quotes

use shared::tee::EnclaveClient;
use tdx_quote::{Quote, QuoteParseError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VerifyError {
    #[error("Could not parse RA report: {0}")]
    QuoteParse(QuoteParseError),
    #[error("Unexpected {0} measurement: \n Expected: {1} \n Got: {2}")]
    UnexpectedMeasurement(&'static str, &'static str, String),
    #[error("RA quote verification failed: {0}")]
    Verification(String),
    #[error("Expected nonce {0}, received nonce: {1}")]
    Nonce(u64, u64),
}

/// TODO: Replace with real values
const MRTD: &str = "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
const RTMR0: &str = "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
const RTMR1: &str = "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

#[derive(Copy, Clone)]
pub struct TdxClient;

impl EnclaveClient for TdxClient {
    type Error = VerifyError;

    fn verify_quote(report: &[u8], nonce: u64) -> Result<[u8; 64], Self::Error> {
        let quote = Quote::from_bytes(report).map_err(VerifyError::QuoteParse)?;
        let mrtd = hex::encode(quote.mrtd());
        let rtmr0 = hex::encode(quote.rtmr0());
        let rtmr1 = hex::encode(quote.rtmr1());
        if mrtd != MRTD {
            return Err(VerifyError::UnexpectedMeasurement("MRTD", MRTD, mrtd));
        }
        if rtmr0 != RTMR0 {
            return Err(VerifyError::UnexpectedMeasurement("RTMR0", RTMR0, rtmr0));
        }
        if rtmr1 != RTMR1 {
            return Err(VerifyError::UnexpectedMeasurement("RTMR1", RTMR1, rtmr1));
        }
        #[cfg(feature = "default")]
        if let Err(e) = quote.verify() {
            return Err(VerifyError::Verification(e.to_string()));
        }

        let ra_nonce =
            u64::from_le_bytes(<[u8; 8]>::try_from(&quote.report_input_data()[32..40]).unwrap());
        if ra_nonce != nonce {
            Err(VerifyError::Nonce(nonce, ra_nonce))
        } else {
            Ok(quote.report_input_data())
        }
    }
}
