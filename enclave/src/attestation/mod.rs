//! This module provides SGX platform related functions like getting local
//! report and transform into a remotely verifiable quote.

use sgx_types::error::SgxStatus;
use sgx_types::types::*;


extern "C" {
    /// Ocall to use sgx_init_quote to init the quote and key_id.
    pub fn ocall_sgx_init_quote(
        ret_val: *mut SgxStatus,
        target_info: *mut TargetInfo,
        group_id: *mut EpidGroupId
    ) -> SgxStatus;

    /// Ocall to get the required buffer size for the quote.
    pub fn ocall_sgx_get_quote_size(
        p_retval: *mut SgxStatus,
        p_sgx_att_key_id: *const AttKeyId,
        p_quote_size: *mut u32,
    ) -> SgxStatus;

    /// Ocall to use sgx_get_quote
    pub fn ocall_sgx_get_quote(
        ret_val: *mut SgxStatus,
        // Signature revocation list
        sigrl: *const u8,
        sigrl_len: u32,
        // report to be signed by Quoting enclave.
        p_report: *const Report,
        quote_type: QuoteSignType,
        p_spid : *const Spid,
        p_nonce: *const QuoteNonce,
        p_qe_report: *mut Report,
        p_quote: *mut u8,
        maxlen: u32,
        p_quote_size: u32,
    ) -> SgxStatus;

    pub fn ocall_get_quote_ecdsa_params(
        ret_val: *mut sgx_status_t,
        p_qe_info: *mut sgx_target_info_t,
        p_quote_size: *mut u32,
    ) -> sgx_status_t;
}

fn make_ias_client_config() -> rustls::ClientConfig {
    let root_store = rustls::RootCertStore::from_iter(
        webpki_roots::TLS_SERVER_ROOTS
            .iter()
            .cloned(),
    );
    let mut config = rustls::ClientConfig::builder();
    config.with_root_certificates(root_store).with_no_client_auth()
}

/// Fetch the list of revoked platforms within the group specifed
/// by the group id.
pub fn get_sigrl_from_intel(fd: c_int, group_id: u32) -> Vec<u8> {
    let config = make_ias_client_config();
}