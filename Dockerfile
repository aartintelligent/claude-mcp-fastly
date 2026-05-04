# syntax=docker/dockerfile:1

# Canonical build:
#   docker build -t aartintelligent/claude-mcp-fastly:latest .

################################################################################
# Build stage — Docker Hardened Image with the Rust toolchain.
#
# `1.95-debian13-dev` is aligned with this crate's MSRV (`Cargo.toml::rust-version`)
# and the runtime stage's Debian 13 base, so the binary's libc/ABI stays
# consistent across stages.
FROM dhi.io/rust:1.95-debian13-dev AS build

WORKDIR /build

# `fastly-api` pulls `reqwest` with default features, which enables `native-tls`
# → links the binary against system OpenSSL. We need its dev headers + pkg-config
# at compile time, and the runtime `.so` at run time (handled in the next stage).
USER root
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev \
 && rm -rf /var/lib/apt/lists/*

# Compile with bind mounts for the source tree (no copy into the image layer)
# and cache mounts for cargo's git db, registry, and target dir — keeps rebuilds
# fast without bloating the build context.
RUN --mount=type=bind,source=src,target=src \
    --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=cache,target=/build/target/ \
    --mount=type=cache,target=/usr/local/cargo/git/db \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    cargo build --locked --release --bin claude-mcp-fastly \
 && cp /build/target/release/claude-mcp-fastly /build/server

################################################################################
# Runtime stage — Docker Hardened Image, minimal Debian 13 base.
#
# Runs as `nonroot` by default, no shell, no package manager. Use `docker debug`
# for troubleshooting (the runtime container itself stays minimal).
FROM dhi.io/debian-base:trixie-debian13 AS final

# Compiled binary.
COPY --from=build /build/server /usr/local/bin/server

# Dynamic libs the binary needs at runtime (OpenSSL via native-tls).
# debian-base is intentionally minimal and does not ship libssl by default;
# copying from the build stage guarantees the same versions used at compile time.
# If you cross-build for arm64, change the multiarch directory to
# `aarch64-linux-gnu` here and below.
COPY --from=build /usr/lib/x86_64-linux-gnu/libssl.so.3 /usr/lib/x86_64-linux-gnu/
COPY --from=build /usr/lib/x86_64-linux-gnu/libcrypto.so.3 /usr/lib/x86_64-linux-gnu/

# Bind on every interface inside the container (the crate default is loopback).
# Port 8000 is well above the privileged range (1024) so nonroot can bind it.
ENV APP_SERVER__HOST=0.0.0.0
ENV APP_SERVER__PORT=8000
EXPOSE 8000

# `APP_FASTLY__API_TOKEN` must be supplied at run time, e.g.:
#   docker run -e APP_FASTLY__API_TOKEN=<token> -p 8000:8000 <image>
# TLS CA certs are already shipped with debian-base, so reqwest can validate
# `api.fastly.com` out of the box.
ENTRYPOINT ["/usr/local/bin/server"]
