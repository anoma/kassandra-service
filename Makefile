
build-enclave:
	cd enclave && \
	cargo osdk build

run-enclave:
	cd enclave && \
	cargo osdk run

.PHONY : build-enclave run-enclave