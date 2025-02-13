
######## SGX SDK Settings ########

SGX_SDK ?= /opt/intel/sgxsdk
SGX_MODE ?= HW
SGX_ARCH ?= x64

TOP_DIR := ./
include $(TOP_DIR)/buildenv.mk

ifeq ($(shell getconf LONG_BIT), 32)
	SGX_ARCH := x86
else ifeq ($(findstring -m32, $(CXXFLAGS)), -m32)
	SGX_ARCH := x86
endif

ifeq ($(SGX_ARCH), x86)
	SGX_COMMON_CFLAGS := -m32
	SGX_LIBRARY_PATH := $(SGX_SDK)/lib
	SGX_BIN_PATH := $(SGX_SDK)/bin/x86
else
	SGX_COMMON_CFLAGS := -m64
	SGX_LIBRARY_PATH := $(SGX_SDK)/lib64
	SGX_BIN_PATH := $(SGX_SDK)/bin/x64
endif

ifeq ($(SGX_DEBUG), 1)
	SGX_COMMON_CFLAGS += -O0 -g
	Rust_Build_Flags :=
	Rust_Build_Out := debug
else
	SGX_COMMON_CFLAGS += -O2
	Rust_Build_Flags := --release
	Rust_Build_Out := release
endif

SGX_EDGER8R := $(SGX_BIN_PATH)/sgx_edger8r
SGX_ENCLAVE_SIGNER := $(SGX_BIN_PATH)/sgx_sign

######## CUSTOM Settings ########

CUSTOM_LIBRARY_PATH := ./lib
CUSTOM_BIN_PATH := ./bins
CUSTOM_SYSROOT_PATH := ./sysroot
CUSTOM_EDL_PATH := $(ROOT_DIR)/sgx_edl/edl
CUSTOM_COMMON_PATH := $(ROOT_DIR)/common

######## EDL Settings ########

Enclave_EDL_Files := enclave/enclave_t.c enclave/enclave_t.h service/enclave_u.c service/enclave_u.h

######## APP Settings ########

App_Rust_Flags := $(Rust_Build_Flags)
Service_Src_Files := $(shell find service/ -type f -name '*.rs') $(shell find service/ -type f -name 'Cargo.toml')
App_Include_Paths := -I ./service -I$(CUSTOM_COMMON_PATH)/inc -I$(CUSTOM_COMMON_PATH)/inc/tlibc -I$(SGX_SDK)/include -I$(CUSTOM_EDL_PATH)
App_C_Flags := $(CFLAGS) $(SGX_COMMON_CFLAGS) -fPIC -Wno-attributes $(App_Include_Paths)

App_Rust_Path := ./service/target/$(Rust_Build_Out)
App_Enclave_u_Object := $(CUSTOM_LIBRARY_PATH)/libenclave_u.a
Service_Name := $(CUSTOM_BIN_PATH)/service

######## Enclave Settings ########

# BUILD_STD=no       use no_std
# BUILD_STD=cargo    use cargo-std-aware
BUILD_STD ?= no

Rust_Build_Target := x86_64-unknown-linux-sgx


ifeq ($(BUILD_STD), cargo)
	Rust_Build_Std := $(Rust_Build_Flags) -Zbuild-std=core,alloc
	Rust_Std_Features :=
	Rust_Target_Flags := --target $(Rust_Target_Path)/$(Rust_Build_Target).json
	Rust_Sysroot_Path := $(CURDIR)/sysroot
	Rust_Sysroot_Flags := RUSTFLAGS="--sysroot $(Rust_Sysroot_Path)"
endif

RustEnclave_Build_Flags := $(Rust_Build_Flags)
RustEnclave_Src_Files := $(shell find enclave/ -type f -name '*.rs') $(shell find enclave/ -type f -name 'Cargo.toml')
RustEnclave_Include_Paths := -I$(SGX_SDK)/include -I$(CUSTOM_COMMON_PATH)/inc/tlibc -I$(CUSTOM_EDL_PATH)

RustEnclave_Link_Libs := -L$(CUSTOM_LIBRARY_PATH) -lenclave
RustEnclave_C_Flags := $(CFLAGS) $(ENCLAVE_CFLAGS) $(SGX_COMMON_CFLAGS) $(RustEnclave_Include_Paths)
RustEnclave_Link_Flags := -Wl,--no-undefined -nostdlib -nodefaultlibs -nostartfiles \
	-Wl,--start-group $(RustEnclave_Link_Libs) -Wl,--end-group \
	-Wl,--version-script=enclave/enclave.lds \
	$(ENCLAVE_LDFLAGS)

ifeq ($(BUILD_STD), cargo)
	RustEnclave_Out_Path := ./enclave/target/$(Rust_Build_Target)/$(Rust_Build_Out)
else
	RustEnclave_Out_Path := ./enclave/target/$(Rust_Build_Out)
endif

RustEnclave_Lib_Name := $(RustEnclave_Out_Path)/libenclave.a
RustEnclave_Name := $(CUSTOM_BIN_PATH)/enclave.so
RustEnclave_Signed_Name := $(CUSTOM_BIN_PATH)/enclave.signed.so

.PHONY: all
all: $(Enclave_EDL_Files) $(Service_Name) $(RustEnclave_Signed_Name)

######## EDL Objects ########

$(Enclave_EDL_Files): $(SGX_EDGER8R) enclave/enclave.edl
	$(SGX_EDGER8R) $(SGX_EDGER8R_MODE) --trusted enclave/enclave.edl --search-path $(CUSTOM_COMMON_PATH)/inc --search-path $(SGX_SDK)/include --search-path $(CUSTOM_EDL_PATH) --trusted-dir enclave
	$(SGX_EDGER8R) $(SGX_EDGER8R_MODE) --untrusted enclave/enclave.edl --search-path $(CUSTOM_COMMON_PATH)/inc --search-path $(SGX_SDK)/include --search-path $(CUSTOM_EDL_PATH) --untrusted-dir service
	@echo "GEN => $(Enclave_EDL_Files)"

######## App Objects ########

service/enclave_u.o: $(Enclave_EDL_Files)
	@$(CC) $(App_C_Flags) -c service/enclave_u.c -o $@

$(App_Enclave_u_Object): service/enclave_u.o
	@mkdir -p $(CUSTOM_LIBRARY_PATH)
	@$(AR) rcsD $@ $^

$(Service_Name): $(App_Enclave_u_Object) service
	@mkdir -p $(CUSTOM_BIN_PATH)
	@cp $(App_Rust_Path)/service $(CUSTOM_BIN_PATH)
	@echo "LINK => $@"

######## Enclave Objects ########

enclave/enclave_t.o: $(Enclave_EDL_Files)
	@$(CC) $(RustEnclave_C_Flags) -c enclave/enclave_t.c -o $@

$(RustEnclave_Name): enclave/enclave_t.o enclave
	@mkdir -p $(CUSTOM_LIBRARY_PATH)
	@mkdir -p $(CUSTOM_BIN_PATH)
	@cp $(RustEnclave_Lib_Name) $(CUSTOM_LIBRARY_PATH)/libenclave.a
	@$(CXX) enclave/enclave_t.o -o $@ $(RustEnclave_Link_Flags)
	@echo "LINK => $@"

$(RustEnclave_Signed_Name): $(RustEnclave_Name) enclave/config.xml
	@$(SGX_ENCLAVE_SIGNER) sign -key enclave/private.pem -enclave $(RustEnclave_Name) -out $@ -config enclave/config.xml
	@echo "SIGN => $@"

######## Build Service ########

.PHONY: service
service:
	@cd service && SGX_SDK=$(SGX_SDK) cargo build $(App_Rust_Flags)

######## Build Enclave ########

.PHONY: enclave
enclave:
ifeq ($(BUILD_STD), cargo)
	@cd $(Rust_Target_Path)/std && cargo build $(Rust_Build_Std) $(Rust_Target_Flags) $(Rust_Std_Features)

	@rm -rf $(Rust_Sysroot_Path)
	@mkdir -p $(Rust_Sysroot_Path)/lib/rustlib/$(Rust_Build_Target)/lib
	@cp -r $(Rust_Target_Path)/std/target/$(Rust_Build_Target)/$(Rust_Build_Out)/deps/* $(Rust_Sysroot_Path)/lib/rustlib/$(Rust_Build_Target)/lib

	@cd enclave && $(Rust_Sysroot_Flags) cargo build $(Rust_Target_Flags) $(RustEnclave_Build_Flags)
else
	@cd enclave && cargo build $(RustEnclave_Build_Flags)
endif

######## Run Enclave ########

.PHONY: run
run: $(Service_Name) $(RustEnclave_Signed_Name)
	@echo -e '\n===== Run Enclave =====\n'
	@cd bin && ./service

.PHONY: clean
clean:
	@rm -f $(Service_Name) $(RustEnclave_Name) $(RustEnclave_Signed_Name) enclave/*_t.* service/*_u.*
	@cd enclave && cargo clean
	@cd service && cargo clean
	@cd $(Rust_Target_Path)/std && cargo clean
	@rm -rf $(CUSTOM_BIN_PATH) $(CUSTOM_LIBRARY_PATH) $(CUSTOM_SYSROOT_PATH)
