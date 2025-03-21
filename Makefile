
build-tdx:
	cd tdx && \
	cargo osdk build

build-mock-tdx:
	cd tdx && \
	cargo osdk build --features "mock"

run-tdx:
	cd tdx && \
	cargo osdk run

build:
	cargo build

build-mock:
	cargo build --no-default-features --features "client/mock"

run-host:
	cargo run --bin host

tdx-all: build-tdx build

tdx-mock: build-mock-tdx build-mock

fmt:
	cargo fmt
	cd enclave && cargo fmt

clippy:
	cargo clippy
	cd tdx && cargo clippy

.PHONY : tdx-all tdx-mock fmt clippy