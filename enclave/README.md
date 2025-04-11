# Enclave

The enclave is the server process tasked with performing FMD on MASP transactions with users' detection keys
and producing updated encrypted index sets of those transactions relevant to said detection keys. It can be
run transparently or inside of a TDX enclave.

Given the restraints coming from a VM based enclave technology, this process is reactive. It waits for messages
from the host and reacts to them. The host and enclave always take turns sending messages, starting with the
host. Thus it is the responsibility of the host to drive the enclave process forward in addition to being a data
availability layer for the enclave process.

The enclave process handles two workflows:
- Registering users' detection keys
- Performing FMD on MASP transactions with registered keys 

## Registering keys

The enclave process can talk to clients via the host. At the start of any such communication, an instance of TLS/RA-TLS must
be negotiated. If the enclave is running transparently, a TLS connection is established. However, if the enclave is running
in a TDX enclave, it performs remote attestation during the TLS negotiation. This allows clients to verify that it is talking
to a TDX enclave and verify the code running therein. This prevents the host process from performing man-in-the-middle
attacks.

Once a secure channel is made between client and enclave, the enclave receives two keys from the client which it stores
in memory: The detection key for FMD and an encryption key for encrypting the results. It should be noted that the 
enclave process has no persistence capabilities. If the process is closed, it will lose all registered keys and they will
need to be registered again. 

## Performing FMD

Each registered key is stored along with a block height, starting from the key's birthday, indicating to which block height
on Namada it is synced to. The host will ask the enclave process for a series of MASP transactions and the enclave
will request those MASP transactions that will advance each key forward by one block.

After receiving these transactions, the enclave will perform FMD and update the index set of MASP transaction relevant
for each key. This updated sets will be encrypted and returned to the host which will persist them in a database.