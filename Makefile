
build-enclave:
	cd enclave && \
	cargo osdk build

run-enclave:
	cd enclave && \
	cargo osdk run

build-host:
	cargo build

run-host:
	cargo run

build: build-enclave build-host

.PHONY : build-enclave run-enclave build-host build run-host