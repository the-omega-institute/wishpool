# syntax=docker/dockerfile:1
# LEAN_IMAGE must be a digest-pinned, prepared image containing the exact
# /opt/lean-workspace and /opt/verifier/.elan/toolchains named by its manifest.
ARG LEAN_IMAGE
FROM rust:1.95-slim-bookworm AS build
WORKDIR /src
COPY api/ ./
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked -p wishpool-verifier
FROM ${LEAN_IMAGE}
COPY --from=build /src/target/release/wishpool-verifier /usr/local/bin/wishpool-verifier
USER 65532:65532
EXPOSE 8081
ENTRYPOINT ["/usr/local/bin/wishpool-verifier"]
