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

## Releases

The release workflow builds installers for Windows x64, Linux x64, macOS
Apple Silicon, and macOS Intel. Push a version tag matching both
`package.json` and `src-tauri/tauri.conf.json` to publish a GitHub Release:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The workflow can also be started manually with an existing or new matching
tag. Release bundles are unsigned unless the repository is configured with
platform signing credentials.
