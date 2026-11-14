# OpenFlex developer entry points.
#
# `make check` is the gate used before a release: formatting, lint, the full
# test suite, and a release build of the CLI.

CARGO ?= cargo
CLI := target/release/openflex

.PHONY: all build test lint fmt fmt-check check examples golden package verify clean

all: check

build:
	$(CARGO) build --release -p openflex-cli

test:
	$(CARGO) test --workspace

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all --check

lint:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

check: fmt-check lint test build
	@echo "check: ok"

examples: build
	@for model in examples/*.ofm; do \
		echo "== $$model"; \
		$(CLI) validate "$$model" || exit 1; \
	done

golden: build
	@for model in examples/golden/*.ofm; do \
		echo "== $$model"; \
		$(CLI) run "$$model" --format json | head -c 200; echo; \
	done

package:
	bash scripts/package-release.sh

verify:
	bash scripts/verify-packaging.sh

clean:
	$(CARGO) clean
