# Client

This binary is for users to interact with third party server's running the Kassandra service. There are two main
functions that this client provides to users:
 - Registering a detection key with service provider(s)
 - Querying an encrypted data blob which contains a set of MASP indices which they should download and trial-decrypt

The client can be build to expect a service running a transparent enclave or one running on TDX. This is configured
via feature flags when building the client binary. If they client is build to expect TDX, it will 
expect the service provider to perform [remote attestation](https://en.wikipedia.org/wiki/Trusted_Computing#Remote_attestation)
to ensure that it is running FMD within a TDX environment and is running the expected code. Otherwise, it will not
communicate further with this server. In transparent mode, these checks are skipped.

## Key registration

Performing FMD is not faster than downloading all MASP transactions and trial-decrypting with a user's secret key. The
benefit of FMD is that it can run as a background service on a third-party server so that user data is ready when requested.

As such, users need to provide a detection key to the services that the FMD process can use to mark MASP transaction as 
relevant to the user or not. This detection key is considered sensitive, but not security critical. The client
will create a TLS connection with each chosen enclave process and use the resulting secure connection to provide a
unique detection key to the enclave.

Users will give the client their "master secret key" which the client will use to derive an appropriate detection key
to send to each service provider. This extraction depends on a few parameters. 

The first parameter to consider is the false positive rate a detection key will have. A higher false-positive rate 
increases the anonymity set of transactions but increases the client side workload of computing balances. There is also
a protocol-wide minimum false positive rate a detection key is allowed to have.

Since users may wish to keep their false-positive rates high for privacy reasons, they can register distinct detection keys with 
multiple service providers. Distinct detection keys will produce different false-positives. This fact can be used client-side
to narrow down the candidate set of MASP  transactions efficiently without providing detection keys with low 
false-positive rates to any service provider. If service providers are non-colluding, this offers better privacy 
guarantees for users. 

Thus users also need to configure which different service providers they will use and the client will handle the 
apportioning of detection keys to them. Thus, before registration happens, the user  populates a config file for the 
client containing information about the chosen service providers.

Users may also provide a birthday for their detection key. This is a block height before which they know that there are no
MASP transactions relevant to them. This birthday will stop the service provider from running FMD on MASP transactions
prior to this block height.

### FMD results

The results of FMD are also considered sensitive, but not security critical. These results are a list of indices pointing
to MASP transactions deemed relevant to a user based on the provided detection key. These results will persisted in 
a database maintained transparently by the host process. 

To ensure the host process does not have access to these results, they are encrypted by the enclave process. This doesn't
add any security unless the enclave process is run inside of TDX, but is always done nevertheless.

The encryption keys are derived by the client using a uuid provided by the server and the "master secret key". They are
thus unique to master key and service provider. The resulting encryption key is also securely transmitted to the enclave
process to be used to encrypt results.

## Querying results

The client also allows users to query the results of FMD performed with their detection key. The client, will compute a
hash of the encryption key that was used to encrypt the results in the host's database. This will be used by the host
to find the encrypted result, which it will return to the client. The client can then decrypt the result. This will
be a set of indices of MASP transactions along with a block height indicating the latest block height FMD was performed
with their detection keys. If multiple services providers are used, the index sets are combined first.