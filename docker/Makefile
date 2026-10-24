##
## Deployment/infrastructure targets
##

# First find out where we are, how we were called, and where to build things
THIS_MAKEFILE    := $(lastword $(MAKEFILE_LIST))
THIS_DIR         := $(shell realpath $(dir $(THIS_MAKEFILE)))
ROOT_DIR         := $(shell realpath $(THIS_DIR)/..)
CALLED_DIRECTLY  := $(if $(START_MAKEFILE),false,true)
DEPLOY_BUILD_DIR := $(ROOT_DIR)/deploy-build

# Get DB info
HEADER_DB_FILE     := headers.db
HEADER_DB_PATH     := $(ROOT_DIR)/$(HEADER_DB_FILE)
HEADER_DB_TMP_PATH := $(DEPLOY_BUILD_DIR)/$(HEADER_DB_FILE)
HEADER_DB_ZST_PATH := $(DEPLOY_BUILD_DIR)/$(HEADER_DB_FILE).zst

# Set Docker build values
DOCKER_IMAGE     ?= zkpow-prover
DOCKER_TAG       ?= latest
DOCKER_PLATFORM  ?= linux/amd64
DOCKER_TARGET    := $(if $(filter mac,$(MAKECMDGOALS)),mac,linux)
DOCKER_FEATURES  := $(if $(filter linux,$(DOCKER_TARGET)),cuda,)

# Setup Docker build prerequisites
DOCKER_BUILD_MARKER := $(DEPLOY_BUILD_DIR)/docker-image.stamp
DOCKER_DEPS_FILE    := $(DEPLOY_BUILD_DIR)/docker.d
DOCKER_DEPS_SCRIPTS := $(filter-out $(THIS_DIR)/README.md $(THIS_DIR)/$(notdir $(THIS_MAKEFILE)),$(wildcard $(THIS_DIR)/*))
DOCKER_DEPS_RS_SRC  := $(ROOT_DIR)/target/release/zkpow-host.d

# DOCKER_DEPS_SCRIPTS := $(filter-out $(THIS_DIR)/README.md $(THIS_DIR)/$(notdir $(THIS_MAKEFILE)),$(wildcard $(THIS_DIR)/*))
##
## Start PHONEY and include-building targets
##
.PHONY: deployment deployment-docker deployment-db deployment-var-% deployment-vars
deployment: deployment-docker deployment-db
deployment-docker: $(DOCKER_BUILD_MARKER)
deployment-db: $(HEADER_DB_ZST_PATH)
deployment-clean:
	rm -rf $(DEPLOY_BUILD_DIR)

# The docker image's prereq are solely for improving staleness checks.
# Trying to require them forces building zkpow-host which we don't want.
# So we include prereqs if/only if the zkpow-host.d file exists which
# enables us the utilize Cargo's dependency info without forcing a build.
ifneq ($(wildcard $(DOCKER_DEPS_RS_SRC)),)
DOCKER_DEPS_SCRIPTS=
-include $(DOCKER_DEPS_FILE)
$(DOCKER_DEPS_FILE): $(DOCKER_DEPS_RS_SRC) | $(DEPLOY_BUILD_DIR)
	@sed -E 's/(.*): (.*)$$/$(DOCKER_BUILD_MARKER): \2/g' $< > $@
endif

##
## Start real, primary targets
##
$(DEPLOY_BUILD_DIR):
	@mkdir -p $@

$(HEADER_DB_ZST_PATH): $(HEADER_DB_PATH) | $(DEPLOY_BUILD_DIR)
	@echo "Compressing $(notdir $<) to $(subst $(CURDIR)/,,$@)"
	@cp $(<) $(HEADER_DB_TMP_PATH)
	@sqlite3 $(HEADER_DB_TMP_PATH) "PRAGMA wal_checkpoint(TRUNCATE);" >/dev/null 2>&1
	@zstd -T0 -3 --rm --no-progress $(HEADER_DB_TMP_PATH) -o $@.tmp >/dev/null 2>&1
	@mv $@.tmp $@
	@echo "Done."


$(DOCKER_BUILD_MARKER): $(DOCKER_DEPS_SCRIPTS) | $(DEPLOY_BUILD_DIR)
	@(cd $(ROOT_DIR) && docker buildx build \
		-f $(THIS_DIR)/Dockerfile.vast-cuda \
		--build-context zkvm-test=$(ROOT_DIR)/../zkvm-test \
		--build-arg ZKPOW_FEATURES="$(DOCKER_FEATURES)" \
		--platform $(DOCKER_PLATFORM) \
		-t $(DOCKER_IMAGE):$(DOCKER_TAG) \
		--load .)
	@touch $@

##
## Add standard target aliases if invoked directly
##
ifeq ($(CALLED_DIRECTLY),true)
.PHONY: all build docker db var-% vars
all: deployment
build: deployment
docker: deployment-docker
db: deployment-db
var-%: deployment-var-%
vars: deployment-vars
endif

##
## Debugging targets
##
deployment-var-%:
	@echo "$* = $($*)"

ALL_VARS := $(shell grep -E '^[[:space:]]*([A-Z_]+)[[:space:]]+[?:=]' $(THIS_MAKEFILE) | sed -E 's/^[[:space:]]*([A-Z_]+)[[:space:]]+[?:=].*$$/\1/g')
deployment-vars:
	@$(foreach v,$(filter-out ALL_VARS,$(ALL_VARS)),$(info $v = $($v)))
	@:



