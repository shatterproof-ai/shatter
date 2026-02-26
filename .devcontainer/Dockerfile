FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04

ARG RUST_VERSION=stable
ARG NODE_VERSION=22
ARG GO_VERSION=1.24.4
ARG GOLANGCI_LINT_VERSION=v1.64.8

# Install OS dependencies (z3 solver libs needed by shatter-core)
RUN add-apt-repository universe \
    && apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libz3-dev \
    libclang-dev \
    cmake \
    tmux \
    && rm -rf /var/lib/apt/lists/*

# Install Rust as vscode user
USER vscode
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --default-toolchain ${RUST_VERSION} \
    --profile minimal \
    && . "$HOME/.cargo/env" \
    && rustup component add clippy rustfmt \
    && echo 'source "$HOME/.cargo/env"' >> ~/.bashrc

ENV PATH="/home/vscode/.cargo/bin:${PATH}"

# Install Node.js
USER root
RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_VERSION}.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Claude Code CLI
RUN npm install -g @anthropic-ai/claude-code

# Install Go
RUN curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz" | tar -C /usr/local -xz
ENV PATH="/usr/local/go/bin:/home/vscode/go/bin:${PATH}"

# Install golangci-lint
RUN curl -sSfL https://raw.githubusercontent.com/golangci/golangci-lint/HEAD/install.sh \
    | sh -s -- -b /usr/local/bin ${GOLANGCI_LINT_VERSION}

# Install Homebrew and beads (bd) issue tracker
USER vscode
RUN NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)" \
    && eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)" \
    && brew install beads \
    && echo 'eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)"' >> ~/.bashrc

ENV PATH="/home/linuxbrew/.linuxbrew/bin:${PATH}"

USER vscode
