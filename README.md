# NIK LPA

NIK LPA is a native Windows eUICC provisioning and RSP engineering application built around the `lpac` / `libeuicc` protocol engine.

The application supports three operating modes:

- **Local LPA** — PC/SC reader and Internet are available on one computer.
- **Card Agent** — owns the PC/SC reader and eUICC and performs ES10b operations without an Internet HTTP backend.
- **Server Agent** — owns the SM-DP+ Internet connection and performs ES9+ operations without a PC/SC backend.

Card Agent and Server Agent can be separated across two Windows computers. The operator manually transfers the current RSP packet between the two application instances. The current engineering build intentionally uses readable `NIKRSP-DEBUG1:` JSON packets so every exchanged field can be inspected while the relay workflow is validated.

## Current relay flow

```text
Card Agent                      Server Agent
   │                                │
   ├── INIT_REQUEST ────────────────►│
   │◄── SERVER_AUTH ─────────────────┤
   ├── EUICC_AUTH ──────────────────►│
   │◄── DOWNLOAD_PREPARE ────────────┤
   ├── EUICC_PREPARED ──────────────►│
   │◄── BOUND_PROFILE_PACKAGE ───────┤
   ├── INSTALL_RESULT ──────────────►│
   │◄── COMPLETE_ACK ─────────────────┤
   │                                │
 COMPLETE                         COMPLETE
```

The Card Agent never needs an HTTP network backend in relay mode. The Server Agent never needs access to the card reader. The same RSP transaction ID and message hash chain are kept across the full store-and-forward flow.

For the full management flowchart, detailed Mermaid sequence, packet table, and operator walkthrough, see [NIK LPA staged RSP relay](docs/NIK-LPA-RSP-RELAY.md).

## Documentation

- [Architecture](docs/NIK-LPA-ARCHITECTURE.md)
- [RSP relay flow and end-to-end walkthrough](docs/NIK-LPA-RSP-RELAY.md)
- [Windows application / developer notes](rust/README.md)
- [Original lpac CLI usage](docs/USAGE.md)
- [Original lpac developer notes](docs/DEVELOPERS.md)

## Build

The Windows application is built in CI. The primary artifact is:

```text
nik-lpa-desktop-windows-x86_64
```

The bundle contains `nik-lpa.exe`, the patched `lpac.exe`, PC/SC and WinHTTP transport drivers, and the required runtime libraries.

The Rust workspace can also be validated with:

```powershell
cd rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo build --release -p lpac-gui --bins
```

## UI

NIK LPA uses the NIK desktop design system with bundled **Roboto** typography, responsive sidebar behavior, fixed content margins, reusable cards and controls, light/dark themes, and a sticky RSP progress view.

## Security note

`NIKRSP-DEBUG1:` is a plaintext debugging transport. It can expose provisioning material such as certificates, signatures, Matching ID, confirmation data, and the Bound Profile Package. It is intended only for controlled engineering validation. Transport encryption can be restored after the staged workflow has been fully validated.

## License and upstream

NIK LPA is based on the open-source `lpac` / `libeuicc` project and keeps the applicable upstream copyright and license metadata in the source tree and `REUSE.toml`.

This repository is licensed under the terms declared by its source files and REUSE metadata. See [REUSE.toml](REUSE.toml) for details.
