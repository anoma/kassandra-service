
build-enclave:
	cd enclave && \
	cargo osdk build

.PHONY : build-enclave