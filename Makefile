TARGET_DIR ?= target
PROFILE ?= release
CARGO_BIN_DIR ?= $(HOME)/.cargo/bin

ZKVM_TEST_RUNNER_PKG := zkvm-test-runner
ZKVM_TEST_RUNNER_SRC := $(shell fd -tf ".+\.rs" crates/$(ZKVM_TEST_RUNNER_PKG)/src)

ZKVM_BINS := cargo-zkvm cargo-zkvm-test zkvm-test-runner
ZKVM_BIN_BUILDS := $(addprefix $(TARGET_DIR)/$(PROFILE)/,$(ZKVM_BINS))
ZKVM_BIN_INSTALLS := $(addprefix $(CARGO_BIN_DIR)/,$(ZKVM_BINS))

.PHONY: build install
install: $(ZKVM_BIN_INSTALLS)
build: $(ZKVM_BIN_BUILDS)

$(CARGO_BIN_DIR)/%: $(TARGET_DIR)/$(PROFILE)/%
	ditto $< $@

$(ZKVM_BIN_BUILDS): $(ZKVM_TEST_RUNNER_SRC)
	cargo build --package $(ZKVM_TEST_RUNNER_PKG) --$(PROFILE)

# $(TARGET_DIR)/$(PROFILE)/%: crates/%/src/main.rs
# 	cargo build -p $(notdir $(@D)) --$(PROFILE)
