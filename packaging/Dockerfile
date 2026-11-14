# syntax=docker/dockerfile:1
#
# Multi-stage build for OpenFlex.
#
# Build from the repository root:
#   docker build -f packaging/Dockerfile -t openflex:dev .
#   docker run --rm openflex:dev --version
#   docker run --rm -v "$PWD/models:/workspace" openflex:dev run /workspace/m1.ofm
#
# The default entrypoint is `openflex`; without arguments it prints `--help`.
#
# The builder image tag can be pinned to a compiler version if the floating
# `rust:1-*` tag is ever too old for rust-version in Cargo.toml:
#   docker build --build-arg RUST_IMAGE=rust:1.100-slim-bookworm ...
ARG RUST_IMAGE=rust:1-slim-bookworm

FROM ${RUST_IMAGE} AS builder

WORKDIR /src

# Only the workspace metadata and the crates are needed to build the CLI.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release -p openflex-cli --locked

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/openflex /usr/local/bin/openflex

# Run as an unprivileged user; models are mounted at /workspace.
RUN useradd --create-home --uid 10001 --shell /usr/sbin/nologin openflex \
    && mkdir -p /workspace \
    && chown openflex:openflex /workspace
USER openflex
WORKDIR /workspace

ENTRYPOINT ["openflex"]
CMD ["--help"]
