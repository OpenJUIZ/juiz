# juiz の開発用コンテナ。
# CI(.github/workflows/test.yml / python.yml)と同じ ubuntu-24.04 に、
# Rust・CMake・Clang・Python 開発ヘッダーと pyjuiz の依存パッケージを入れる。
FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN apt-get update \
    && export DEBIAN_FRONTEND=noninteractive \
    && apt-get install -y --no-install-recommends \
        build-essential cmake clang pkg-config jq \
        python3 python3-dev python3-pip python3-venv \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

# 埋め込み Python(PyO3)から使う pyjuiz の依存パッケージ(CI と同じもの)
RUN python3 -m pip install --break-system-packages --no-cache-dir \
        pyyaml httpx pillow docstring_parser

# Rust は stable を vscode ユーザーに入れる(CI の dtolnay/rust-toolchain@stable と同じ)。
# Apple Silicon の Mac でも動くように、アーキテクチャに合った rustup-init を取得する。
USER vscode
RUN case "$(uname -m)" in \
        x86_64) target=x86_64-unknown-linux-gnu ;; \
        aarch64) target=aarch64-unknown-linux-gnu ;; \
        *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;; \
    esac \
    && curl --proto '=https' --tlsv1.2 -sSf -o /tmp/rustup-init \
        "https://static.rust-lang.org/rustup/dist/${target}/rustup-init" \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --profile minimal --default-toolchain stable \
        --component rustfmt,clippy,rust-src \
    && rm /tmp/rustup-init
ENV PATH=/home/vscode/.cargo/bin:${PATH}
USER root
