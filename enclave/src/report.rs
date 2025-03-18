use tdx_quote::{Quote, SigningKey};

#[cfg(feature = "mock")]
pub(crate) fn get_quote(report_data: [u8; 64]) -> Quote {
    let attestation_key = SigningKey::from_slice(&[1; 32]).unwrap();
    let pck_key = SigningKey::from_slice(&[2; 32]).unwrap();
    Quote::mock(attestation_key, pck_key, report_data, alloc::vec![])
}

#[cfg(not(feature = "mock"))]
pub(crate) fn get_quote(report_data: [u8; 64]) -> Quote {
    todo!()
}
