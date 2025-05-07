# Kassandra Service

This project provides both and client and server binaries for a service that allows users to delegate the work
of determining which MASP txs are relevant to them to a untrusted third party. The server makes use of so-called
[Fuzzy Message Detection](https://eprint.iacr.org/2021/089) as well as the potential to use secure enclaves to add
extra security for users.

## Background

User's [MASP](https://github.com/anoma/masp) transactions are stored as encrypted notes on the [Namada](https://github.com/anoma/namada) blockchain. 
The knowledge of which of these notes belong to which user is considered private data. For users to determine which notes belong to them, the naive approach
is to download all encrypted notes and then see which of these they can decrypt with their secret keys.

This solution does not scale well and requires both large amounts of network bandwidth and compute time. This is not 
ideal for many users, especially those wishing to view their balances on constrained devices. The goal of fuzzy message
detection (henceforth, FMD) is to restrict the number of MASP txs users need to download and trial-decrypt. 

The reason FMD is called "fuzzy" is because it should detect all transactions relevant to a user in addition to a number 
of false positives according to a configurable rate. These false-positives act as "cover traffic" for the user's 
transactions. FMD is designed so that it can be performed by untrusted third parties on behalf of users. 

## Components

This repo contains both server and [client](./client) code. The server is further divided into two processes: the [host](./host) and 
[enclave](./enclave). The ability to restrict the anonymity set of a user's transactions is considered sensitive, although
not security critical. As such, it is possible to entrust the data necessary to perform FMD to a [trusted execution 
environment](https://en.wikipedia.org/wiki/Trusted_execution_environment) (henceforth, TEE) via an encrypted channel.

The client supports checking attestations of the TEE running on the server and talking to it securely. However, servers
may also choose to not use TEE's and run the FMD logic in a normal process. The generic FMD logic is found in the [enclave](./enclave)
library crate, while two other crates implement it [transparently](./transparent) and on [TDX](./tdx). 

