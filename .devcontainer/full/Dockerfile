FROM node:latest

ARG TZ
ENV TZ="$TZ"

LABEL org.opencontainers.image.title="dcx Development Container" \
      org.opencontainers.image.description="Development environment for dcx (Colima workspace mounting wrapper)" \
      org.opencontainers.image.authors="dcx-team" \
      project="dcx" \
      purpose="development"

# Install development tools (removed: build-essential, man-db, nano, vim)
RUN apt-get update && apt-get install -y --no-install-recommends \
  git \
  gh \
  curl \
  wget \
  less \
  zsh \
  fzf \
  procps \
  sudo \
  bindfs \
  iptables \
  ipset \
  iproute2 \
  dnsutils \
  aggregate \
  jq \
  && apt-get clean && rm -rf /var/lib/apt/lists/*

# Install Rust toolchain to a shared location
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable && \
  /root/.cargo/bin/rustup component add clippy rustfmt && \
  cp -r /root/.cargo /opt/rust-tools && \
  chmod -R 755 /opt/rust-tools

# Configure the existing 'node' user (UID 1000) for development
# Rename group and user, set shell
RUN groupmod -n rust node && \
  usermod -l rust node && \
  usermod -d /home/rust -m rust && \
  usermod -s /bin/zsh rust && \
  echo "rust ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Persist shell history
RUN mkdir -p /commandhistory && \
  touch /commandhistory/.bash_history /commandhistory/.zsh_history && \
  chown -R rust:rust /commandhistory

# Set `DEVCONTAINER` environment variable
ENV DEVCONTAINER=true

# Create workspace directory
RUN mkdir -p /workspace && \
  chown -R rust:rust /workspace

WORKDIR /workspace

# Install git-delta for better diffs
ARG GIT_DELTA_VERSION=0.18.2
RUN ARCH=$(dpkg --print-architecture) && \
  wget "https://github.com/dandavison/delta/releases/download/${GIT_DELTA_VERSION}/git-delta_${GIT_DELTA_VERSION}_${ARCH}.deb" && \
  dpkg -i "git-delta_${GIT_DELTA_VERSION}_${ARCH}.deb" && \
  rm "git-delta_${GIT_DELTA_VERSION}_${ARCH}.deb"

# Install firewall script and grant rust user passwordless sudo access to it
COPY init-firewall.sh /usr/local/bin/init-firewall.sh
RUN chmod +x /usr/local/bin/init-firewall.sh && \
  echo "rust ALL=(root) NOPASSWD: /usr/local/bin/init-firewall.sh" > /etc/sudoers.d/rust-firewall && \
  chmod 0440 /etc/sudoers.d/rust-firewall

# Switch to non-root user
USER rust

# Set up shell environment
ENV SHELL=/bin/zsh
ENV EDITOR=nano
ENV VISUAL=nano
# Add Rust tools and Claude Code to PATH
ENV PATH="/home/rust/.local/bin:/opt/rust-tools/bin:$PATH"

# Install Claude Code via native installer
RUN curl -fsSL https://claude.ai/install.sh | bash

# Install shell theme and plugins
ARG ZSH_IN_DOCKER_VERSION=1.2.0
RUN sh -c "$(wget -O- https://github.com/deluan/zsh-in-docker/releases/download/v${ZSH_IN_DOCKER_VERSION}/zsh-in-docker.sh)" -- \
  -p git \
  -p fzf \
  -a "source /usr/share/doc/fzf/examples/key-bindings.zsh" \
  -a "source /usr/share/doc/fzf/examples/completion.zsh" \
  -a "export PROMPT_COMMAND='history -a' && export HISTFILE=/commandhistory/.zsh_history" \
  -x
