# Host

The server running the Kassandra service has two components: the host and enclave. While it is possible to run
 the enclave component in a trusted execution environment, the host always runs in a transparent process. As such,
it is not considered a trusted component of the service. 

The host has four main functions:

 - Provide an API for clients
 - Handle communication to/from the enclave process
 - Persist data
 - Continuously fetch MASP transactions from the blockchain

Since the enclave library is designed to be compatible with trusted execution environments, it is not intended to be
able to connect to clients directly. Instead, all communication must pass through the host process. Part of its job is to
listen for incoming client requests and forward them on to the enclave process if necessary. Since the host is not a
trusted process, the client and enclave must negotiate a secure TLS connection in order to share sensitive information. 

The host is also in charge of fetching all MASP transactions. It does this continuously as a background process by
querying a MASP indexer. When starting the host for the first time, a url for a MASP indexer must be provided. Afterwards,
it will be persisted in a config file found under the `.kassandra` directory in your home folder.

The MASP transactions are persisted in an SQLite database, also kept in the `.kassandra` directory. When it is not handling
client requests, the host will ask the enclave process which MASP transactions it would like to perform FMD upon. 

The host makes these transactions available to the enclave which will update the indices of relevant MASP transactions for each
registered key, and provide the encrypted results back to the host.

The host maintains a second SQLite database for storing these encrypted indices. A client can query the host for the 
entries from this database. 

## Security

If the enclave is running transparently, the host gains the same trust assumptions as the enclave. This means it can see
all detection keys and will be able to decrypt the entries in the database that it maintains. 

If the enclave is run inside of TDX, then the host can truly be said to be an untrusted component. The only means of attack
that it can perform is attempts to censor data by not making MASP transactions available to the enclave, refusing to respond
 to clients, etc. 