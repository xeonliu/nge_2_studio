# NGE2 ISO Studio

NGE2 ISO Studio is a read-only desktop inspector for the PSP game
`ULJS00061` / `ULJS00064`. It opens an ISO in place and lazily reads ISO9660,
HGAR, HGPT and EVS data without extracting the disc image.

## Development

Prerequisites: Rust stable, Node.js 22+, Yarn 1.x, and the platform dependencies
required by Tauri 2.

```sh
yarn install
yarn tauri dev
```

The browser-only development view uses deterministic sample data so the
workbench can be tested without an ISO:

```sh
yarn dev
```

## Verification

```sh
cargo test --workspace
yarn typecheck
yarn test:e2e
yarn tauri build
```

No game data is distributed with this repository. Tests construct minimal
binary fixtures at runtime.
