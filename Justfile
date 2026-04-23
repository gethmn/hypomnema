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

run *ARGS:
    cargo run --bin hmn -- {{ARGS}}
