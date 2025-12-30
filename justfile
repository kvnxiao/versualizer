fmt:
    cargo fmt

lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo fmt --check

dev:
    cargo run -p versualizer-app-dioxus

test:
    cargo test --workspace
