FROM rust:latest

ARG TZ
ENV TZ="$TZ"

# Install development tools
RUN apt-get update && apt-get install -y --no-install-recommends \
  build-essential \
  git \
  gh \
  curl \
  wget \
  less \
  nano \
  vim \
  zsh \
  fzf \
  procps \
  sudo \
  man-db \
  && apt-get clean && rm -rf /var/lib/apt/lists/*

# Create non-root user for development
ARG USERNAME=rust
ARG USER_UID=1000
ARG USER_GID=$USER_UID
RUN groupadd --gid $USER_GID $USERNAME && \
  useradd --uid $USER_UID --gid $USER_GID -m -s /bin/zsh $USERNAME && \
  echo "$USERNAME ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Persist shell history
RUN mkdir -p /commandhistory && \
  touch /commandhistory/.bash_history /commandhistory/.zsh_history && \
  chown -R $USERNAME:$USERNAME /commandhistory

# Set `DEVCONTAINER` environment variable
ENV DEVCONTAINER=true

# Create workspace directory
RUN mkdir -p /workspace && \
  chown -R $USERNAME:$USERNAME /workspace

WORKDIR /workspace

# Install git-delta for better diffs
ARG GIT_DELTA_VERSION=0.18.2
RUN ARCH=$(dpkg --print-architecture) && \
  wget "https://github.com/dandavison/delta/releases/download/${GIT_DELTA_VERSION}/git-delta_${GIT_DELTA_VERSION}_${ARCH}.deb" && \
  dpkg -i "git-delta_${GIT_DELTA_VERSION}_${ARCH}.deb" && \
  rm "git-delta_${GIT_DELTA_VERSION}_${ARCH}.deb"

# Switch to non-root user
USER $USERNAME

# Set up shell environment
ENV SHELL=/bin/zsh
ENV EDITOR=nano
ENV VISUAL=nano

# Install shell theme and plugins
ARG ZSH_IN_DOCKER_VERSION=1.2.0
RUN sh -c "$(wget -O- https://github.com/deluan/zsh-in-docker/releases/download/v${ZSH_IN_DOCKER_VERSION}/zsh-in-docker.sh)" -- \
  -p git \
  -p fzf \
  -a "source /usr/share/doc/fzf/examples/key-bindings.zsh" \
  -a "source /usr/share/doc/fzf/examples/completion.zsh" \
  -a "export PROMPT_COMMAND='history -a' && export HISTFILE=/commandhistory/.zsh_history" \
  -x

# Ensure Rust toolchain is up to date
RUN rustup update && \
  rustup component add clippy rustfmt
