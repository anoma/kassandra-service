tdx-build-tool:
	chmod +x tdx/create_build_tool.sh && \
  	tdx/create_build_tool.sh

build-tdx: tdx-build-tool
	cd tdx && \
	env RUST_BACKTRACE=1 cargo-osdk osdk build --release --target-profile $(shell readlink -f tdx/x86_64-unknown-none.json)

build-mock-tdx: tdx-build-tool
	cd tdx && \
	env RUST_BACKTRACE=1 cargo-osdk osdk build --release --features "mock" --target-profile $(shell readlink -f tdx/x86_64-unknown-none.json)

run-tdx: tdx-build-tool build-tdx
	cd tdx && \
	env RUST_BACKTRACE=1 cargo-osdk osdk run --release

run-mock-tdx: tdx-build-tool build-mock-tdx
	cp /usr/share/OVMF/OVMF_VARS.fd tdx/
	cd tdx && \
	env RUST_BACKTRACE=1 cargo osdk run --release --features "mock"

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