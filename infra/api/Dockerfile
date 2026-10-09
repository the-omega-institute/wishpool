# syntax=docker/dockerfile:1
# Build context: repository root. One image serves both roles (api, worker);
# the worker compiles uploaded LaTeX as arXiv does, so the runtime carries
# TeX Live (pdfLaTeX, XeLaTeX, LuaLaTeX, BibTeX and the common packages).
FROM rust:1.95-slim-bookworm AS build
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends cmake clang pkg-config && rm -rf /var/lib/apt/lists/*
COPY api/ ./
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p wishpool \
 && cp target/release/wishpool /usr/local/bin/wishpool

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
      texlive-latex-base texlive-latex-recommended texlive-latex-extra \
      texlive-fonts-recommended texlive-fonts-extra texlive-science texlive-publishers \
      texlive-bibtex-extra texlive-plain-generic texlive-xetex texlive-luatex cm-super lmodern \
 && rm -rf /var/lib/apt/lists/* /usr/share/doc/* \
 && groupadd --gid 65532 wishpool && useradd --uid 65532 --gid 65532 --no-create-home --shell /usr/sbin/nologin wishpool
COPY --from=build /usr/local/bin/wishpool /usr/local/bin/wishpool
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/wishpool"]
