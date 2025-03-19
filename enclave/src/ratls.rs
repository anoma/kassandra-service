use fmd::fmd2_compact::CompactSecretKey;
use shared::ratls::Connection;
use shared::tee::{EnclaveComm, EnclaveRNG, RemoteAttestation};
use shared::{AckType, MsgFromHost, MsgToHost};

use crate::Ctx;

/// Create a new TLS connection and add it to the list of active
/// connections.
///
/// Creates a Remote Attestation report which signs over its ephemeral
/// public key and a session id. This is sent to the client for verification
pub(crate) fn register_key<RA, COM, RNG>(
    mut ctx: Ctx<RA, COM, RNG>,
    pk: x25519_dalek::PublicKey,
    nonce: u64,
) where
    RA: RemoteAttestation,
    COM: EnclaveComm,
    RNG: EnclaveRNG,
{
    // create a new connection and get the public ephemeral key
    let conn = Connection::new(ctx.rng);
    let enclave_pk = if let Connection::Handshake { ephemeral_key } = &conn {
        x25519_dalek::PublicKey::from(ephemeral_key)
    } else {
        unreachable!()
    };

    // initialize the connection and compute shared key
    let conn = if let Ok(conn) = conn.initialize(pk) {
        conn
    } else {
        ctx.com
            .write_client_err("Failed to initialize TLS connection.");
        return;
    };

    // generate Remote Attestation report
    let mut report_data = [0u8; 64];
    for (ix, b) in enclave_pk
        .to_bytes()
        .into_iter()
        .chain(nonce.to_le_bytes().into_iter())
        .enumerate()
    {
        report_data[ix] = b;
    }

    // send the quote to the client for verification
    let quote = ctx.ra.get_quote(report_data);
    ctx.com.write(&MsgToHost::RATLS { report: quote });

    // wait for acknowledgement from the client
    let Ok(MsgFromHost::RATLSAck(ack)) = ctx.com.read() else {
        ctx.com.write_err("Received unexpected message");
        return;
    };
    let AckType::Success(cipher) = ack else {
        return;
    };
    let fmd_key: CompactSecretKey = match conn.decrypt_msg(&cipher) {
        Ok(key) => {
            ctx.com.write(&MsgToHost::KeyRegSuccess);
            key
        }
        Err(e) => {
            ctx.com
                .write_client_err(&format!("Error receiving fmd key: {e}"));
            return;
        }
    };
}
