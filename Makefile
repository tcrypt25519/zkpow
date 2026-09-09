TARGET_DIR ?= target
PROFILE ?= release

CARGO_BIN_DIR ?= $(HOME)/.cargo/bin

ZKVM_TEST_RUNNER_PKG ?= zkvm-test-runner
ZKVM_TEST_RUNNER_BIN ?= cargo-zkvm-test
ZKVM_TEST_RUNNER_SRC := $(shell fd -tf ".+\.rs" crates/$(ZKVM_TEST_RUNNER_PKG)/src)
ZKVM_TEST_RUNNER_BIN_PATH := $(TARGET_DIR)/$(PROFILE)/$(ZKVM_TEST_RUNNER_BIN)
ZKVM_TEST_RUNNER_BIN_DST := $(CARGO_BIN_DIR)/$(ZKVM_TEST_RUNNER_BIN)

# List of all binary packages to build
ALL_MAINS := $(wildcard crates/**/src/main.rs)

ALL_BINS := $(TARGET_DIR)/$(PROFILE)/${ZKVM_TEST_RUNNER_BIN}
# ALL_BINS := $(shell rg --no-filename -A 2 "\[\[bin\]\]" crates | grep 'name = "' | sed -E 's/^name = "([^"]+?)"$/\1/g')

# TARGET_BIN = $(TARGET_DIR)/$(PROFILE)/$(ZKVM_TEST_RUNNER_BIN)

.PHONY: build install
.DEFAULT: install

build: $(ALL_BINS)

install: $(ZKVM_TEST_RUNNER_BIN_DST)

$(ZKVM_TEST_RUNNER_BIN_DST): $(ZKVM_TEST_RUNNER_BIN_PATH)
	ditto $(ZKVM_TEST_RUNNER_BIN_PATH) $(ZKVM_TEST_RUNNER_BIN_DST)

$(ZKVM_TEST_RUNNER_BIN_PATH): $(ZKVM_TEST_RUNNER_SRC)
	cargo build --package $(ZKVM_TEST_RUNNER_PKG) --$(PROFILE)

# $(TARGET_DIR)/$(PROFILE)/%: crates/%/src/main.rs
# 	cargo build -p $(notdir $(@D)) --$(PROFILE)