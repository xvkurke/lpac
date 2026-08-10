# NIK LPA repository guide

## Purpose

This document is the repository-level map for NIK LPA. It explains which part of the tree owns each responsibility, where protocol logic lives, how the Windows application is built, and where future embedded/Card Agent work should integrate.

NIK LPA is intentionally split into two major implementation domains:

1. the existing C `lpac` / `libeuicc` protocol engine;
2. the NIK Rust Windows application and staged relay orchestration.

The C implementation remains authoritative for SGP.22 protocol encoding, ES9+, ES10b, ASN.1, certificates, APDU and BPP processing. Rust owns the product UI, process supervision, staged relay state model and application-level validation.

## Repository map

```text
.
├── .github/
│   ├── workflows/
│   │   ├── build.yaml          Windows x86_64 C runtime build only
│   │   ├── rust-gui.yml       Windows desktop application/bundle build
│   │   └── ...                lint and upstream checks
│   └── scripts/               CI toolchain/build helpers
│
├── docs/
│   ├── NIK-LPA-ARCHITECTURE.md
│   ├── NIK-LPA-RSP-RELAY.md
│   ├── NIK-LPA-REPOSITORY-GUIDE.md
│   ├── NIK-LPA-STM32-EUICC.md
│   ├── NIK-LPA-GLOSSARY.md
│   └── upstream lpac documentation
│
├── src/
│   ├── applet/
│   │   ├── relay.c            staged Card Agent / Server Agent C adapter
│   │   ├── relay.h
│   │   └── profile/           standard profile management/download applets
│   ├── CMakeLists.txt
│   └── main.c                 lpac application entry and applet registration
│
├── utils/
│   └── lpac/                  utility/runtime support
│
├── rust/
│   ├── Cargo.toml             workspace
│   ├── README.md
│   ├── apps/
│   │   └── lpac-gui/
│   │       └── src/bin/
│   │           ├── nik-lpa.rs
│   │           └── nik_lpa/
│   │               ├── app.rs
│   │               ├── controller.rs
│   │               ├── logic.rs
│   │               ├── model.rs
│   │               ├── theme.rs
│   │               ├── views.rs
│   │               └── widgets.rs
│   ├── crates/
│   │   ├── lpac-core/
│   │   ├── lpac-backend/
│   │   └── lpac-relay-backend/
│   └── tools/
│       └── fake-lpac/
│
├── README.md                  NIK product landing page
└── REUSE.toml                 license/upstream attribution metadata
```

## C protocol domain

### `src/applet/relay.c`

This is the key bridge for staged RSP operation. It turns `libeuicc` operations into a long-lived line-oriented JSON API over stdin/stdout.

Card Agent operations:

- `card.init`
- `card.authenticateServer`
- `card.prepareDownload`
- `card.installBpp`
- `card.cancel`
- `shutdown`

Server Agent operations:

- `server.initiateAuthentication`
- `server.authenticateClient`
- `server.getBpp`
- `server.cancel`
- `shutdown`

The relay applet does not replace `libeuicc`. It only maps JSON fields to the existing ES9+/ES10b function calls and maps returned protocol structures back to JSON.

### Card-side C calls

The current staged flow depends on the following card-side operations:

```text
es10b_get_euicc_challenge_r
es10b_get_euicc_info_r
es10b_authenticate_server_r
es10b_prepare_download_r
es10b_load_bound_profile_package_r
es10b_cancel_session_r
```

A fresh standard Profile List operation is then used after BPP loading to independently confirm the expected ICCID.

### Server-side C calls

The current staged flow depends on:

```text
es9p_initiate_authentication_r
es9p_authenticate_client_r
es9p_get_bound_profile_package_r
es9p_cancel_session_r
```

These are executed only by the Server Agent process in relay mode.

## Rust product domain

### `lpac-core`

Owns transport-independent application data structures and validation:

- activation code parsing and redaction;
- relay packet model;
- relay stages and direction;
- job UUID;
- target EID hash;
- timestamps and expiry;
- previous-message hash chain;
- transaction ID continuity;
- legacy encrypted envelope primitives retained for future secure transport work.

The active debug relay uses `NIKRSP-DEBUG1:` plaintext packets; the old encrypted implementation is not the active GUI path.

### `lpac-backend`

Runs one-shot `lpac.exe` commands for operations where a persistent staged relay helper is unnecessary. Examples include:

- reader discovery;
- chip information/EID;
- Profile List;
- Local LPA download.

### `lpac-relay-backend`

Supervises one long-lived `lpac relay card-agent` or `lpac relay server-agent` child process.

It is responsible for:

- child process lifecycle;
- line-oriented JSON requests/responses;
- request UUID correlation;
- operation-name validation;
- stderr capture;
- graceful shutdown;
- Windows console suppression;
- role-specific backend environment.

Role isolation:

| Role | `LPAC_APDU` | `LPAC_HTTP` |
|---|---|---|
| Card Agent | `pcsc` | `stdio` |
| Server Agent | `stdio` | `winhttp` |

### `lpac-gui` / `nik-lpa.exe`

`nik-lpa.exe` is the main Windows application.

Module ownership:

- `app.rs`: long-lived application state;
- `controller.rs`: worker thread and command/event boundary;
- `logic.rs`: business/RSP orchestration;
- `model.rs`: relay session model and debug packet encoding/validation;
- `views.rs`: page composition and responsive layout;
- `theme.rs`: NIK colors, spacing, typography and embedded Roboto;
- `widgets.rs`: reusable UI controls/components.

Blocking `lpac` operations must not execute directly on the egui UI thread.

## End-to-end runtime paths

### Local LPA

```text
nik-lpa.exe
  -> lpac-backend
  -> lpac.exe
  -> libeuicc
  -> PC/SC -> eUICC
  -> WinHTTP -> SM-DP+
```

### Staged Card Agent

```text
nik-lpa.exe
  -> lpac-relay-backend
  -> lpac relay card-agent
  -> libeuicc ES10b
  -> PC/SC
  -> eUICC
```

No normal Internet HTTP backend is owned by the Card Agent in relay mode.

### Staged Server Agent

```text
nik-lpa.exe
  -> lpac-relay-backend
  -> lpac relay server-agent
  -> libeuicc ES9+
  -> WinHTTP / TLS
  -> SM-DP+
```

No physical smart-card reader is owned by the Server Agent in relay mode.

## Windows-only build policy

NIK product CI builds and publishes Windows artifacts only.

### C runtime workflow

`.github/workflows/build.yaml` now keeps one C build target:

```text
Windows x86_64 with MinGW
```

Linux, Linux ARM, QMI variants, Windows ARM and macOS entries were removed from the NIK product build matrix. This does not require deleting portable source code; it only removes unsupported product build targets from CI/artifact production.

### Desktop workflow

`.github/workflows/rust-gui.yml` builds the production Windows desktop bundle with:

- Rust format/clippy/tests;
- release Rust binaries;
- native Windows UCRT64 `lpac.exe`;
- PC/SC, stdio and WinHTTP driver DLLs;
- required runtime DLLs;
- isolated bundle smoke test.

Primary artifact:

```text
nik-lpa-desktop-windows-x86_64
```

## Security ownership

Do not confuse the following layers:

1. `NIKRSP-DEBUG1` is currently plaintext application transport.
2. ES9+ uses HTTPS/TLS between Server Agent and SM-DP+.
3. SGP.22 certificate/signature validation is part of the RSP protocol and is ultimately enforced by SM-DP+/eUICC according to the relevant operation.
4. The Bound Profile Package contains RSP-protected profile data; NIK LPA does not extract operator subscription secrets such as Ki as plaintext fields.

## Embedded/Card Agent direction

The future STM32 implementation should not port the full Windows application or the full Server Agent. It should replace only the Card Agent execution side.

Target responsibility:

```text
external transport
    -> STM32 Card Agent state machine
    -> ES10b subset
    -> APDU HAL
    -> modem UICC tunnel or direct ISO7816
    -> eUICC
```

The required API, electrical contacts, modem/eUICC variants and memory considerations are documented in `NIK-LPA-STM32-EUICC.md`.

## Documentation maintenance rule

When protocol behavior changes, update documentation in the same change set:

- repository ownership/layout -> this file;
- architecture boundaries -> `NIK-LPA-ARCHITECTURE.md`;
- packet/function/data/certificate sequence -> `NIK-LPA-RSP-RELAY.md`;
- embedded hardware/Card Agent contract -> `NIK-LPA-STM32-EUICC.md`;
- terminology -> `NIK-LPA-GLOSSARY.md`.
