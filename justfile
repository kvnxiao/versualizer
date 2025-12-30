lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

dev:
    cargo run -p versualizer-app-dioxus

test:
    cargo test --workspace
