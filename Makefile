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
PROFILE     ?= release
TARGET_DIR  ?= target
PROFILE_DIR ?= $(if $(filter dev test,$(PROFILE)),debug,$(if $(filter release bench,$(PROFILE)),release,$(PROFILE)))
BUILD_DIR   := $(CURDIR)/$(TARGET_DIR)/$(PROFILE_DIR)

# Cargo metadata
CARGO_TOML    := Cargo.toml
CARGO_LOCK    := Cargo.lock
CRATE_CARGOS  := $(wildcard $(CRATES_DIR)/*/$(CARGO_TOML))

# Binary names and paths
BIN_NAMES  := $(if $(BINS),$(BINS),$(shell awk '/^\[/ { in_pkg = ($$0 == "\[\[bin\]\]") } in_pkg && /^name[ \t]*=/ { split($$0, a, "\""); print a[2] }' $(CRATES_DIR)/**/Cargo.toml))
BIN_BUILDS := $(addprefix $(BUILD_DIR)/,$(BIN_NAMES))
BIN_DEPS   := $(addsuffix .d, $(BIN_BUILDS))
$(info BIN_NAMES = $(BIN_NAMES))
$(info BIN_BUILDS = $(BIN_BUILDS))
$(info BIN_DEPS = $(BIN_DEPS))

##
## Ergonomic entrypoint targets
##
.PHONY: all build clean var-%
.DEFAULT_GOAL := build

all: build

build: $(CARGO_LOCK) $(BIN_BUILDS)

clean:
	cargo clean --target-dir $(TARGET_DIR)

include $(BIN_DEPS)

##
## Actual build rules
##
$(BUILD_DIR)/%:
	@echo "Building $(*:.d=)"
	cargo build --bin $(*:.d=) --profile $(PROFILE) --target-dir $(TARGET_DIR)

$(CARGO_LOCK): $(CARGO_TOML) $(CRATE_CARGOS)
	cargo generate-lockfile

var-%:
	@echo "$* = ${$*}"

##
## Docker image
##

DOCKER_IMAGE    ?= zkpow-prover:latest
DOCKER_PLATFORM ?= linux/amd64
DOCKER_FEATURES ?= cuda
DOCKER_STAMP    := .docker-image.stamp
DOCKER_DEPS     := docker/Dockerfile.vast-cuda docker/entrypoint.sh docker/supervisord.conf \
                   $(CARGO_TOML) $(CARGO_LOCK) \
                   $(shell find $(CRATES_DIR) -type f \( -name '*.rs' -o -name 'Cargo.toml' \))

.PHONY: docker
docker: $(DOCKER_STAMP)

$(DOCKER_STAMP): $(DOCKER_DEPS)
	docker buildx build --platform $(DOCKER_PLATFORM) -f docker/Dockerfile.vast-cuda \
		--build-arg ZKPOW_FEATURES="$(DOCKER_FEATURES)" -t $(DOCKER_IMAGE) --load .
	touch $@
