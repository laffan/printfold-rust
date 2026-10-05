Begin with README.md to get a sense of the project and docs/README.md to get a sense of how it is built. If you are asked to revise or augment a specific feature that is documented in the docs folder, read the relevant documentation only.

Repo rules:

1. Code files should be no larger than ~700 lines.
2. Code should not diverge from the architecture set out by docs/README.md (if it needs to, check in first). Layout, project files and PDF belong in `crates/printfold-core`; native capabilities go through `src-tauri` commands and `src/services/bridge.ts`.
3. Code should be modular, comprehensible and easy to maintain.
4. Keep feature parity with the original app in mind; record intentional differences in docs/parity.md.

Checks before committing: `cargo test --workspace`, `cargo clippy --workspace`, `npm run build`.
