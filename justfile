fmt:
    cargo fmt

lint:
    cargo clippy --workspace --all-targets --all-features
    cargo fmt --check

dev:
    cargo run -p versualizer-app-dioxus

test:
    cargo test --workspace

bundle:
    cd versualizer-app-dioxus && dx bundle --features bundle
