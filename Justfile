default:
    @just --list

check:
    cargo check --all-targets

test:
    cargo nextest run

lint:
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt --all

watch:
    bacon

# Build recipes
build-debug:
    cargo build --bins

build-release:
    cargo build --release --bins

# Install recipes (copies to ~/.local/bin)
install:
    install -Dm755 target/release/hmn ~/.local/bin/hmn
    install -Dm755 target/release/hmnd ~/.local/bin/hmnd

install-dev:
    install -Dm755 target/debug/hmn ~/.local/bin/hmn
    install -Dm755 target/debug/hmnd ~/.local/bin/hmnd

# Convenience recipes
build:
    just build-release

dev:
    just build-debug install-dev

release:
    just build-release install
