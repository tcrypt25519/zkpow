# =============================================================================
# Generic Cargo Workspace Makefile
#
# Supports:
# - Multi-binary crates (e.g. src/bin/<name>.rs, src/bin/<name>/main.rs)
# - Single-binary crates (src/main.rs where crate name == bin name)
# - Root/single-crate repositories (src/bin/*.rs or src/main.rs)
# - Automatic discovery or explicit configuration via PACKAGES and/or BINS
# =============================================================================

# Fix for MacOS 27 lld
export MACOSX_DEPLOYMENT_TARGET=27.0

# Packages to target (optional; defaults to all crates under $(CRATES_DIR))
PACKAGES    ?=

# Workspace layout and build configuration
CRATES_DIR  ?= crates
PROFILE     ?= release
TARGET_DIR  ?= target
INSTALL_DIR ?= $(HOME)/.local/bin
PROFILE_DIR ?= $(if $(filter dev test,$(PROFILE)),debug,$(if $(filter release bench,$(PROFILE)),release,$(PROFILE)))
BUILD_DIR   := $(TARGET_DIR)/$(PROFILE_DIR)

# Cargo metadata
CARGO_TOML       := Cargo.toml
CARGO_LOCK       := Cargo.lock
ROOT_CARGO       := $(CARGO_TOML)
ROOT_CARGO_FILES := $(ROOT_CARGO) $(CARGO_LOCK)
ALL_CRATE_CARGOS  = $(wildcard $(CRATES_DIR)/*/$(CARGO_TOML))

# Crate search directories
CRATE_SEARCH_DIRS := $(strip $(if $(PACKAGES),$(addprefix $(CRATES_DIR)/,$(PACKAGES)),$(wildcard $(CRATES_DIR)/*)))

# Auto-discover binaries from crates or repository root:
# 1. <crate>/src/bin/*.rs
# 2. <crate>/src/bin/*/main.rs
# 3. <crate>/src/main.rs
# 4. src/bin/*.rs / src/main.rs (root crate)
DISCOVERED_BINS := $(sort \
  $(notdir $(basename $(wildcard $(addsuffix /src/bin/*.rs,$(CRATE_SEARCH_DIRS))))) \
  $(notdir $(patsubst %/src/main.rs,%,$(wildcard $(addsuffix /src/main.rs,$(CRATE_SEARCH_DIRS))))) \
  $(notdir $(patsubst %/main.rs,%,$(wildcard $(addsuffix /src/bin/*/main.rs,$(CRATE_SEARCH_DIRS))))) \
  $(notdir $(basename $(wildcard src/bin/*.rs))) \
  $(if $(wildcard src/main.rs),$(notdir $(CURDIR))) \
)

# Binaries to build (defaults to discovered binaries, or specify explicitly: e.g. BINS ?= foo bar)
BINS ?= $(DISCOVERED_BINS)

# Locate the primary source file defining binary $(1)
BIN_SRC = $(firstword $(wildcard \
  $(addsuffix /src/bin/$(1).rs,$(CRATE_SEARCH_DIRS) $(wildcard $(CRATES_DIR)/*)) \
  $(addsuffix /src/bin/$(1)/main.rs,$(CRATE_SEARCH_DIRS) $(wildcard $(CRATES_DIR)/*)) \
  $(CRATES_DIR)/$(1)/src/main.rs \
  $(addsuffix /src/main.rs,$(CRATE_SEARCH_DIRS) $(wildcard $(CRATES_DIR)/*)) \
  src/bin/$(1).rs \
  src/bin/$(1)/main.rs \
  src/main.rs \
))

# Extract the crate directory containing binary $(1) (or override via BIN_CRATE_<bin>)
BIN_CRATE_DIR = $(strip $(if $(BIN_CRATE_$(1)),$(BIN_CRATE_$(1)),\
  $(patsubst ./%,%,$(firstword $(subst /src/, ,./$(call BIN_SRC,$(1)))))\
))

# Extract package name for `cargo --package` (or override via BIN_PKG_<bin>)
BIN_PKG = $(strip $(if $(BIN_PKG_$(1)),$(BIN_PKG_$(1)),\
  $(if $(filter-out . $(notdir $(CURDIR)),$(notdir $(call BIN_CRATE_DIR,$(1)))),$(notdir $(call BIN_CRATE_DIR,$(1))))\
))

# Prerequisites for binary $(1): all .rs files in its crate + crate Cargo.toml + root files
CRATE_SRCS    = $(if $(call BIN_CRATE_DIR,$(1)),$(shell [ -d "$(call BIN_CRATE_DIR,$(1))/src" ] && find $(call BIN_CRATE_DIR,$(1))/src -name '*.rs'))
CRATE_CARGO   = $(if $(call BIN_CRATE_DIR,$(1)),$(wildcard $(call BIN_CRATE_DIR,$(1))/$(CARGO_TOML)))
CRATE_PREREQS = $(call CRATE_SRCS,$(1)) $(call CRATE_CARGO,$(1)) $(ROOT_CARGO_FILES)

BUILT     := $(addprefix $(BUILD_DIR)/,$(BINS))
INSTALLED := $(addprefix $(INSTALL_DIR)/,$(BINS))

.PHONY: all install clean
.SECONDARY: $(BUILT)
.SECONDEXPANSION:

all: $(BUILT)

install: $(INSTALLED)

clean:
	cargo clean --target-dir $(TARGET_DIR)

$(INSTALL_DIR)/%: $(BUILD_DIR)/%
	@mkdir -p $(@D)
	install -m 755 $< $@

$(BUILD_DIR)/%: $$(call CRATE_PREREQS,$$*)
	cargo build $(if $(call BIN_PKG,$*),--manifest-path crates/$(call BIN_PKG,$*)/Cargo.toml) --bins --profile $(PROFILE) --target-dir $(TARGET_DIR)

$(CARGO_LOCK): $(ROOT_CARGO) $(ALL_CRATE_CARGOS)
	cargo generate-lockfile
