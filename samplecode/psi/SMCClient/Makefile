# Licensed to the Apache Software Foundation (ASF) under one
# or more contributor license agreements.  See the NOTICE file
# distributed with this work for additional information
# regarding copyright ownership.  The ASF licenses this file
# to you under the Apache License, Version 2.0 (the
# "License"); you may not use this file except in compliance
# with the License.  You may obtain a copy of the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing,
# software distributed under the License is distributed on an
# "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
# KIND, either express or implied.  See the License for the
# specific language governing permissions and limitations
# under the License.

######## SGX SDK Settings ########
SGX_SDK ?= /opt/intel/sgxsdk
SGX_MODE ?= HW
SGX_ARCH ?= x64

ifeq ($(shell getconf LONG_BIT), 32)
	SGX_ARCH := x86
else ifeq ($(findstring -m32, $(CXXFLAGS)), -m32)
	SGX_ARCH := x86
endif

ifeq ($(SGX_ARCH), x86)
	SGX_COMMON_CFLAGS := -m32
	SGX_LIBRARY_PATH := $(SGX_SDK)/lib
	SGX_ENCLAVE_SIGNER := $(SGX_SDK)/bin/x86/sgx_sign
	SGX_EDGER8R := $(SGX_SDK)/bin/x86/sgx_edger8r
else
	SGX_COMMON_CFLAGS := -m64
	SGX_LIBRARY_PATH := $(SGX_SDK)/lib64
	SGX_ENCLAVE_SIGNER := $(SGX_SDK)/bin/x64/sgx_sign
	SGX_EDGER8R := $(SGX_SDK)/bin/x64/sgx_edger8r
endif

ifeq ($(SGX_DEBUG), 1)
ifeq ($(SGX_PRERELEASE), 1)
$(error Cannot set SGX_DEBUG and SGX_PRERELEASE at the same time!!)
endif
endif

ifeq ($(SGX_DEBUG), 1)
	SGX_COMMON_CFLAGS += -O0 -g
else
	SGX_COMMON_CFLAGS += -O2
endif

ifeq ($(SUPPLIED_KEY_DERIVATION), 1)
	SGX_COMMON_CFLAGS += -DSUPPLIED_KEY_DERIVATION
endif


######## App Settings ########
ifneq ($(SGX_MODE), HW)
	Urts_Library_Name := sgx_urts_sim
else
	Urts_Library_Name := sgx_urts
endif

App_Cpp_Files := isv_app/isv_app.cpp ../Util/LogBase.cpp ../Networking/NetworkManager.cpp \
../Networking/Session.cpp ../Networking/Client.cpp ../Networking/Server.cpp isv_app/MessageManager.cpp ../Networking/NetworkManagerClient.cpp \
../GoogleMessages/Messages.pb.cpp ../Networking/AbstractNetworkOps.cpp ../Util/UtilityFunctions.cpp ../WebService/WebService.cpp \
../Util/Base64.cpp

App_Include_Paths := -Iworker -I$(SGX_SDK)/include -Iheaders -I../Util -I../Networking -Iisv_app -I../GoogleMessages -I/usr/local/include -I../WebService

App_C_Flags := $(SGX_COMMON_CFLAGS) -fPIC -Wno-attributes $(App_Include_Paths)

# Three configuration modes - Debug, prerelease, release
#   Debug - Macro DEBUG enabled.
#   Prerelease - Macro NDEBUG and EDEBUG enabled.
#   Release - Macro NDEBUG enabled.
ifeq ($(SGX_DEBUG), 1)
	App_C_Flags += -DDEBUG -UNDEBUG -UEDEBUG
else ifeq ($(SGX_PRERELEASE), 1)
	App_C_Flags += -DNDEBUG -DEDEBUG -UDEBUG
else
	App_C_Flags += -DNDEBUG -UEDEBUG -UDEBUG
endif

App_Cpp_Flags := $(App_C_Flags) -std=c++11 -DEnableClient
App_Link_Flags := $(SGX_COMMON_CFLAGS) -L$(SGX_LIBRARY_PATH) -l$(Urts_Library_Name) -L. -lsgx_ukey_exchange -lpthread -lworker \
-Wl,-rpath=$(CURDIR)/sample_libcrypto -Wl,-rpath=$(CURDIR) -llog4cpp -lboost_system -L/usr/lib -lssl -lcrypto -lboost_thread -lprotobuf -L /usr/local/lib -ljsoncpp -lcurl

ifneq ($(SGX_MODE), HW)
	App_Link_Flags += -lsgx_uae_service_sim
else
	App_Link_Flags += -lsgx_uae_service
endif

App_Cpp_Objects := $(App_Cpp_Files:.cpp=.o)

App_Name := app



######## Worker Settings ########
Worker_Cpp_Files := worker/ecp.cpp ../Util/LogBase.cpp \
worker/ias_ra.cpp ../Util/UtilityFunctions.cpp ../WebService/WebService.cpp worker/sha256.cpp worker/Worker.cpp \

Worker_Include_Paths := -I$(SGX_SDK)/include -I$(SGX_SDK)/include/tlibc -I$(SGX_SDK)/include/stlport -Isample_libcrypto

Worker_C_Flags := $(SGX_COMMON_CFLAGS) -fPIC -Wno-attributes -I$(SGX_SDK)/include -Isample_libcrypto -I/usr/local/include -I../GoogleMessages -I../Util \
-I../WebService -I../Networking

Worker_Cpp_Flags := $(Worker_C_Flags) -std=c++11
Worker_Link_Flags :=  -shared $(SGX_COMMON_CFLAGS) -L$(SGX_LIBRARY_PATH) -lsample_libcrypto -Lsample_libcrypto -llog4cpp

Worker_Cpp_Objects := $(Worker_Cpp_Files:.cpp=.o)

.PHONY: all run

all: libworker.so $(App_Name)



######## App Objects ########
isv_app/%.o: isv_app/%.cpp
	@$(CXX) $(App_Cpp_Flags) -c $< -o $@
	@echo "CXX  <=  $<"

../Util/%.o: ../Util/%.cpp
	@$(CXX) $(App_Cpp_Flags) -c $< -o $@
	@echo "CXX  <=  $<"

../Networking/%.o: ../Networking/%.cpp
	@$(CXX) $(App_Cpp_Flags) -c $< -o $@
	@echo "CXX  <=  $<"

../GoogleMessages/%.o: ../GoogleMessages/%.cpp
	@$(CXX) $(App_Cpp_Flags) -c $< -o $@
	@echo "CXX  <=  $<"

../WebService/%.o: ../WebService/%.cpp
	@$(CXX) $(App_Cpp_Flags) -c $< -o $@
	@echo "CXX  <=  $<"

$(App_Name): $(App_Cpp_Objects)
	@$(CXX) $^ -o $@ $(App_Link_Flags)
	@echo "LINK =>  $@"


######## Worker Objects ########
worker/%.o: worker/%.cpp
	@$(CXX) $(Worker_Cpp_Flags) -c $< -o $@
	@echo "CXX  <=  $<"

libworker.so: $(Worker_Cpp_Objects)
	@$(CXX) $^ -o $@ $(Worker_Link_Flags)
	@echo "LINK =>  $@"

.PHONY: clean

clean:
	@rm -f $(App_Name) $(Enclave_Name) $(Signed_Enclave_Name) $(App_Cpp_Objects) isv_app/isv_enclave_u.* $(Enclave_Cpp_Objects) isv_enclave/isv_enclave_t.* libworker.* $(Worker_Cpp_Objects)



