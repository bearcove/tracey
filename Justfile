list:
    just --list

lint:
    (cd crates/tracey/src/bridge/http/dashboard && pnpm exec tsgo --noEmit) && echo "TypeScript's okay"
    cargo check && echo "Rust's okay"
