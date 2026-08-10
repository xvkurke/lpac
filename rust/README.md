# NIK LPA Windows application

NIK LPA is a native Windows desktop application built around the existing `lpac` / `libeuicc` protocol engine.

The current application supports three modes:

- **Local LPA** — standard one-computer profile management and download.
- **Card Agent** — PC/SC + eUICC side of staged RSP.
- **Server Agent** — Internet + SM-DP+ side of staged RSP.

The primary executable is `nik-lpa.exe`.

For repository-wide documentation start at [`docs/README.md`](../docs/README.md).

## Current staged relay

The relay uses long-lived C helper processes:

```text
lpac relay card-agent
lpac relay server-agent
```

The current engineering transport is deliberately readable:

```text
NIKRSP-DEBUG1:<JSON>
```

This allows every field exchanged between the eUICC side and the SM-DP+ side to be inspected during hardware debugging.

The eight stages are:

```text
INIT_REQUEST
SERVER_AUTH
EUICC_AUTH
DOWNLOAD_PREPARE
EUICC_PREPARED
BOUND_PROFILE_PACKAGE
INSTALL_RESULT
COMPLETE_ACK
```

See [the detailed relay document](../docs/NIK-LPA-RSP-RELAY.md) for functions, input/output data, certificate roles, APDU communication, validation and BPP installation.

## Runtime isolation

Card Agent:

```text
LPAC_APDU=pcsc
LPAC_HTTP=stdio
```

Server Agent:

```text
LPAC_APDU=stdio
LPAC_HTTP=winhttp
```

This keeps the reader side and Internet side separated at the transport-driver level.

## Rust workspace

- `lpac-core` — activation parsing, relay packet/state validation, legacy secure transport helpers.
- `lpac-backend` — one-shot local `lpac` process adapter and verification helpers.
- `lpac-relay-backend` — long-lived Card/Server helper supervision.
- `lpac-gui` — Windows UI and orchestration.
- `fake-lpac` — deterministic process-level tests.

The primary GUI is organized into:

- `app.rs` — owned application state;
- `controller.rs` — background worker boundary;
- `logic.rs` — workflow orchestration;
- `model.rs` — relay-session model and debug codec;
- `views.rs` — page composition;
- `theme.rs` — NIK colors, Roboto typography, spacing and geometry tokens;
- `widgets.rs` — reusable UI primitives.

See [the architecture document](../docs/NIK-LPA-ARCHITECTURE.md) and [repository guide](../docs/NIK-LPA-REPOSITORY-GUIDE.md).

## UI design contract

The Windows UI uses:

- bundled Roboto typography;
- 14 px primary text;
- 44 px controls;
- 24 px standard card padding;
- 16 px compact card padding;
- fixed outer page gutters;
- responsive 260 / 60 px sidebar;
- maximum 830 px content rail;
- NIK red for brand/navigation emphasis;
- blue for primary actions;
- coordinated light and dark palettes.

Do not introduce page-local colors, arbitrary radii, or ad-hoc control heights. Reuse `theme.rs` and `widgets.rs`.

## Windows-only build policy

NIK product CI produces Windows artifacts only. Portable upstream source can remain, but Linux/Linux ARM/QMI, Windows ARM and macOS product artifacts are not part of the active NIK build matrix.

See [Windows build pipeline](../docs/NIK-LPA-WINDOWS-BUILD.md).

## Build and test

```powershell
cd rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo build --release -p lpac-gui --bins
```

Repository CI also builds the C runtime, assembles the native Windows UCRT64 bundle, and runs an isolated bundle smoke test.

## Windows bundle

CI publishes:

```text
nik-lpa-desktop-windows-x86_64
```

The bundle contains:

- `nik-lpa.exe`;
- patched `lpac.exe`;
- PC/SC and WinHTTP transport drivers;
- required runtime libraries;
- diagnostic binaries used during development.

Extract the complete ZIP into one directory. Do not copy only the EXE files.

## Hardware validation

The core acceptance scenario is two application instances:

1. Card Agent has the real PC/SC reader and eUICC.
2. Server Agent has Internet access to the real SM-DP+.
3. The operator transfers every `NIKRSP-DEBUG1` packet manually.
4. Card Agent loads the real BPP.
5. A fresh Profile List confirms the expected ICCID.
6. Server Agent receives `INSTALL_RESULT`.
7. Card Agent receives `COMPLETE_ACK`.
8. Both sides report completion.

The future STM32 implementation should replace only the Card Agent execution side. See [STM32/eUICC integration](../docs/NIK-LPA-STM32-EUICC.md).

## Security note

The active `NIKRSP-DEBUG1` transport is plaintext by design and is for controlled debugging only. It may expose provisioning material. The secure transport layer should be restored after the staged workflow is finalized.
