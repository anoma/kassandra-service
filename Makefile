
build-tdx:
	cd tdx && \
	env RUST_BACKTRACE=1 OSDK_TARGET=$(shell readlink -f tdx/x86_64-unknown-none.json) cargo osdk build --release

build-mock-tdx:
	cd tdx && \
	env RUST_BACKTRACE=1 OSDK_TARGET=$(shell readlink -f tdx/x86_64-unknown-none.json) cargo osdk build --scheme tdx --release --features "mock"

run-tdx:
	cd tdx && \
	env RUST_BACKTRACE=1 OSDK_TARGET=$(shell readlink -f tdx/x86_64-unknown-none.json) cargo osdk run --release

run-mock-tdx:
	cp /usr/share/OVMF/OVMF_VARS.fd tdx/
	cd tdx && \
	env RUST_BACKTRACE=1 OSDK_TARGET=$(shell readlink -f tdx/x86_64-unknown-none.json) cargo osdk run --release --features "mock"

build:
	cargo build

build-mock:
	cargo build --no-default-features --features "client/mock"

run-enclave:
	cargo run --bin transparent

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