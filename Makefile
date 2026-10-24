MAKEFLAGS += -r --no-print-directory
export MACOSX_DEPLOYMENT_TARGET=27.0
# =============================================================================
# Generic Cargo Workspace Makefile
#
# Supports:
# - Multi-binary crates (e.g. src/bin/<name>.rs, src/bin/<name>/main.rs)
# - Single-binary crates (src/main.rs where crate name == bin name)
# - Root/single-crate repositories (src/bin/*.rs or src/main.rs)
# - Automatic discovery or explicit configuration via PACKAGES and/or BINS
# =============================================================================

##
## Caller settable options
##

# Binaries to build (optional; defaults to all binaries in the workspace)
BINS ?=

##
## Fixed and calculated values
##

# Workspace layout and build configuration
CRATES_DIR  ?= crates
TARGET_DIR  ?= target
PROFILE     ?= $(if $(filter dev,$(MAKECMDGOALS)),dev,release)
PROFILE_DIR ?= $(if $(filter dev test,$(PROFILE)),debug,$(if $(filter release bench,$(PROFILE)),release,$(PROFILE)))
BUILD_DIR   := $(CURDIR)/$(TARGET_DIR)/$(PROFILE_DIR)

# Cargo metadata
CARGO_LOCK  := Cargo.lock
MANIFESTS   := $(wildcard Cargo.toml Cargo.lock */Cargo.toml $(CRATES_DIR)/**/Cargo.toml)

# Binary names and paths
BIN_NAMES  := $(if $(BINS),$(BINS),$(shell awk '/^\[/ { in_pkg = ($$0 == "\[\[bin\]\]") } in_pkg && /^name[ \t]*=/ { split($$0, a, "\""); print a[2] }' $(CRATES_DIR)/**/Cargo.toml))
BIN_BUILDS := $(addprefix $(BUILD_DIR)/,$(BIN_NAMES))
BIN_DEPS   := $(addsuffix .d, $(BIN_BUILDS))

ifneq ($(VERBOSE),)
$(info BIN_NAMES = $(BIN_NAMES))
$(info BIN_BUILDS = $(BIN_BUILDS))
$(info BIN_DEPS = $(BIN_DEPS))
endif

##
## Ergonomic entrypoint targets
##
.PHONY: all build clean var-%
.DEFAULT_GOAL := build

all: build

build: $(MANIFESTS) $(BIN_BUILDS)

clean: deployment-clean
	cargo clean --target-dir $(TARGET_DIR)

ifneq ($(wildcard $(BIN_BUILDS)/*.d),)
$(info Including $$(BIN_DEPS))
include $(BIN_DEPS)
endif

##
## Actual build rules
##
$(BUILD_DIR)/%:
	@echo "Building $(*:.d=)"
	cargo build --bin $(*:.d=) --profile $(PROFILE) --target-dir $(TARGET_DIR)

$(CARGO_LOCK): $(filter-out $(CARGO_LOCK),$(MANIFESTS))
	cargo generate-lockfile

var-%:
	@echo "$* = ${$*}"

##
## Include Deploy Make targets
##

THIS_MAKEFILE := $(realpath $(lastword $(MAKEFILE_LIST)))
START_MAKEFILE ?= $(realpath $(firstword $(MAKEFILE_LIST)))

-include docker/Makefile
