FROM rust:slim-bookworm

LABEL org.opencontainers.image.title="dcx Development Container (Slim)" \
      org.opencontainers.image.description="Lightweight development environment for dcx" \
      project="dcx" \
      purpose="development"

RUN apt-get update && apt-get install -y --no-install-recommends \
  git \
  bindfs \
  pkg-config \
  && apt-get clean && rm -rf /var/lib/apt/lists/*

# clippy + rustfmt (not in slim image)
RUN rustup component add clippy rustfmt

# Non-root user
RUN useradd -m -s /bin/bash dev
RUN mkdir -p /workspace && chown dev:dev /workspace

ENV DEVCONTAINER=true

USER dev
WORKDIR /workspace
