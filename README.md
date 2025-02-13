# Kassandra Service

The service behind that runs the Kassandra protocol. This protocol implements fuzzy message detection for the Namada 
MASP utilizing trusted execution environments.

This service runs the following components:
 * A MASP indexer w/ backing Postgres database
 * A scanning algorithm to perform fuzzy message detection on MASP notes with registered detection keys. Results are
   stored (encrypted) in a database.
 * A user facing API.

The fuzzy message detection algorithm is run inside an SGX enclave. This service
also provides TEE related functionalities, such as remote attestation.

## SGX Environment Setup
This service can only be run on machines with Intel processors. If the processor is not SGX-capable, the service can be
run in simulation mode, but **this is for debugging puporses only**. Furthermore, this service is designed to be run only
on linux machines.

Setting up an SGX (Software Guard Extensions) environment involves installing the necessary tools, SDKs, and dependencies 
to develop and run SGX-enabled applications. Below is a step-by-step guide to set up the environment on **Linux**.

---

### **1. Prerequisites**

- A CPU that supports Intel SGX (e.g., recent Intel processors).
- SGX must be enabled in the BIOS/UEFI settings.
- OS support:
    - **Linux**: Ubuntu, CentOS, or other supported distributions.

### **2. Hardware check**
To run SGX applications, a hardware with Intel SGX support is needed. You can check with this list of [supported hardware](https://github.com/ayeks/SGX-hardware). Note that you sometimes need to configure BIOS to enable SGX.

* You can check if SGX is enabled on you system by compiling an running `test_sgx.c` from this project: 
  [SGX-hardware](https://github.com/ayeks/SGX-hardware). It will produce a report.

### **3. Setup for Linux Environment**

The subsequent instructions are applicable to Ubuntu 20.04. You can find the official [installation guides](https://download.01.org/intel-sgx/sgx-linux/2.17.1/docs/) for Intel SGX software on the 01.org website. Additionally, you may find guides for other OS versions and SGX versions at [Intel-sgx-docs](https://download.01.org/intel-sgx/).

#### Step 1: Install SGX Driver
```shell
wget https://download.01.org/intel-sgx/sgx-linux/2.17.1/distro/ubuntu20.04-server/sgx_linux_x64_driver_2.11.b6f5b4a.bin

sudo ./sgx_linux_x64_driver_2.11.b6f5b4a.bin

ls /dev/isgx 
```

#### Step2: Install SGX PSW
* Add the repository to your sources:
```shell
echo 'deb [arch=amd64] https://download.01.org/intel-sgx/sgx_repo/ubuntu focal main' | sudo tee /etc/apt/sources.list.d/intel-sgx.list
```

* Add the key to the list of trusted keys used by the apt to authenticate packages:
```shell
wget -qO - https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key | sudo apt-key add -
```

* Update the apt and install the packages:
```shell
sudo apt-get update
```

* Install launch service: 
```shell
sudo apt-get install libsgx-launch libsgx-urts 
``` 

* Install EPID-based attestation service: 
```shell
sudo apt-get install libsgx-epid libsgx-urts  
```

* Install algorithm agnostic attestation service: 
```shell
sudo apt-get install libsgx-quote-ex libsgx-urts
```

#### Step3: Install SGX SDK
```shell
wget https://download.01.org/intel-sgx/sgx-linux/2.17.1/distro/ubuntu20.04-server/sgx_linux_x64_sdk_2.17.101.1.bin

./sgx_linux_x64_sdk_2.17.101.1.bin

source /your_path/sgxsdk/environment
```

#### Step4: Verify and test your SGX Setup
 1. Compile a sample SGX project (available in the SDK: `/your_sdk_path/sgxsdk/SampleCode`).
 2. Run it in both hardware and simulation modes to verify SGX functionality.
    - **Simulation mode**: SGX programs can run on CPUs without SGX support.
    - **Hardware mode**: Requires an SGX-enabled processor and BIOS.

### Building this project

The Makefile makes certain assumptions that may need to be configured differently depending on your setup.

It firstly assumes that SGX SDK has been installed at `/opt/intel/sgxsdk`. This path can be altered either in the 
Makefile directly or by setting the `SGX_SDK` environment variable to the correct path.

Secondly, if you wish to run this library in simulation mode, you can either set `SGX_MODE=SIM` or change the Makefile 
directly. By default, this variable is set to hardware mode (`SGX_MODE=HW`).

You can build this project in debug mode by setting `SGX_DEBUG=1`, otherwise both C and Rust parts will be built in release
mode.

The project is also `no-std` by default. This can be changed by set `BUILD_STD=cargo` or changing the Makefile directly. 
However, we don't recommend this.
