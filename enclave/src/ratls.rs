use fmd::fmd2_compact::CompactSecretKey;
use rand_core::{CryptoRng, Error, RngCore};
use shared::ratls::Connection;
use shared::{AckType, MsgFromHost, MsgToHost};

use crate::com::HostCom;
use crate::report::get_quote;

struct Rng;

impl RngCore for Rng {
    fn next_u32(&mut self) -> u32 {
        let r = self.next_u64().to_le_bytes();
        u32::from_le_bytes([r[0], r[1], r[2], r[3]])
    }

    fn next_u64(&mut self) -> u64 {
        ostd::arch::x86::read_random().unwrap()
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        let mut ix = 0;
        let mut r = self.next_u64().to_le_bytes().to_vec();

        while let Some(b) = dst.get_mut(ix) {
            if r.is_empty() {
                r = self.next_u64().to_le_bytes().to_vec();
            }
            *b = r.pop().unwrap();
            ix += 1;
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        Ok(self.fill_bytes(dest))
    }
}

impl CryptoRng for Rng {}

/// Create a new TLS connection and add it to the list of active
/// connections.
///
/// Creates a Remote Attestation report which signs over its ephemeral
/// public key and a session id. This is sent to the client for verification
pub(crate) fn register_key(pk: x25519_dalek::PublicKey, nonce: u64) {
    // create a new connection and get the public ephemeral key
    let mut conn = Connection::new(Rng);
    let enclave_pk = if let Connection::Handshake { ephemeral_key } = &conn {
        x25519_dalek::PublicKey::from(ephemeral_key)
    } else {
        unreachable!()
    };

    // initialize the connection and compute shared key
    let conn = if let Ok(conn) = conn.initialize(pk) {
        conn
    } else {
        HostCom::write_client_err("Failed to initialize TLS connection.");
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
    let quote = get_quote(report_data);
    HostCom::write(MsgToHost::RATLS {
        report: quote.as_bytes(),
    });

    // wait for acknowledgement from the client
    let Ok(MsgFromHost::RATLSAck(ack)) = HostCom::read() else {
        HostCom::write_err("Received unexpected message")
    };
    let AckType::Success(cipher) = ack else {
        return;
    };
    let fmd_key: CompactSecretKey = match conn.decrypt_msg(&cipher) {
        Ok(key) => {
            HostCom::write(MsgToHost::KeyRegSuccess);
            key
        }
        Err(e) => {
            HostCom::write_client_err("Could not decrypt / deserialize message");
            return;
        }
    };
}
