
build-enclave:
	cd enclave && \
	cargo osdk build

build-mock-enclave:
	cd enclave && \
	cargo osdk build --features "mock"

run-enclave:
	cd enclave && \
	cargo osdk run

build-host:
	cargo build

build-mock-host:
	cargo build --no-default-features --features "client/mock"

run-host:
	cargo run

build: build-enclave build-host

build-mock: build-mock-enclave build-mock-host

fmt:
	cargo fmt
	cd enclave && cargo fmt

clippy:
	cargo clippy
	cd enclave && cargo clippy

.PHONY : build build-mock fmt clippy