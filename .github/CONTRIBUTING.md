# Contributing

Thanks for considering a contribution. The lowest-friction path is **a new scaffold**: pick an app you use, write a TOML manifest, open a PR.

## Dev environment

- Rust 1.80+ (`rustup default stable`)
- Node 20.19+ (or Node 22.12+), pnpm 9+
- Tauri prerequisites: <https://tauri.app/start/prerequisites/>
- On Windows, install the WebView2 runtime if missing.

```bash
pnpm install
pnpm tauri dev          # run desktop app
cargo test --workspace  # test Rust crates
pnpm lint               # eslint + prettier check
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

## Branching & commits

- Branch from `main`, named `feat/...`, `fix/...`, `scaffold/<app>`.
- Sign off your commits (`git commit -s`); we use [DCO](https://developercertificate.org/) instead of a CLA.
- Keep PRs focused. New scaffolds = one PR per app.

## Writing a scaffold

See [docs/SCAFFOLD-AUTHORING.md](docs/SCAFFOLD-AUTHORING.md). The PR template asks for:

- detection paths
- risk classification (`low` / `medium` / `high`)
- a screenshot of the scaffold matching on a real machine
- explicit confirmation that no message database / private data is touched

## Code review

- Small PRs are reviewed within ~48h.
- We squash-merge.
- CI must be green.

## Reporting issues

Please use the issue templates. For security issues see [SECURITY.md](SECURITY.md) – do **not** open a public issue.
