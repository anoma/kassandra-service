
tdx-build-tool:
	cp /usr/share/OVMF/OVMF_VARS.fd tdx/
	cd tdx/build-tdx && \
	cargo build -r

build-tdx: tdx-build-tool
	env RUST_BACKTRACE=1 ./build-tdx/target/release/build-tdx --target $(shell readlink -f tdx/x86_64-unknown-none.json) --release build

build-mock-tdx: tdx-build-tool
	env RUST_BACKTRACE=1 ./build-tdx/target/release/build-tdx --target $(shell readlink -f tdx/x86_64-unknown-none.json) --release --features "mock" build

run-tdx: tdx-build-tool
	env RUST_BACKTRACE=1 ./build-tdx/target/release/build-tdx --target $(shell readlink -f tdx/x86_64-unknown-none.json) --release --features "mock" run

run-mock-tdx: tdx-build-tool
	env RUST_BACKTRACE=1 ./build-tdx/target/release/build-tdx --target $(shell readlink -f tdx/x86_64-unknown-none.json) --release --features "mock" run

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