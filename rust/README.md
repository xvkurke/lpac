# NIK LPA Rust migration

This directory contains the incremental Rust replacement for the desktop-facing parts of lpac.

## Current architecture

- `lpac-core`: activation-code parsing, secret redaction, activation jobs, and authenticated encrypted transfer envelopes.
- `lpac-backend`: adapter around the existing C lpac/libeuicc implementation.
- `lpac-gui`: native egui/eframe desktop application for PC/SC reader testing.

The proven SGP.22 implementation remains in C during the first phase. Replacing it module-by-module is safer than rewriting authentication, ASN.1, ES8+, ES9+, and ES10x simultaneously.

## Encrypted transfer string

The GUI creates a portable string with this format:

```text
NIKLPA1:<base64url authenticated envelope>
```

The payload is encrypted with XChaCha20-Poly1305. The transfer key must be delivered to the receiving device through a separate trusted channel. The envelope contains the exact activation string, optional confirmation code, reader hint, EID hint, job ID, and timestamp.

The activation string is never included in application logs. UI output uses redacted identifiers.

> This first implementation uses a shared 256-bit transfer key for the lab. Production device enrollment should replace it with recipient public-key encryption or a device-bound key stored in TPM/secure storage.

## Build

```bash
cd rust
cargo run -p lpac-gui
```

Place the current `lpac` executable next to the GUI or set its path in the application.

## Secure stdin contract

The Rust backend invokes:

```text
lpac profile download -a -
```

and writes the activation code to stdin. A small companion change in the C CLI is required so that `-a -` reads one line from stdin. Until that change lands, chip info and profile listing work, but profile installation through the GUI is intentionally blocked by the legacy CLI contract.

## Migration stages

1. Rust GUI, job model, encryption envelope, and legacy backend.
2. Secure stdin/FD activation input and structured progress events in C lpac.
3. Native Rust PC/SC transport and profile management API.
4. Port ES10x and ASN.1 boundaries behind compatibility tests.
5. Port ES9+/HTTP state machine and remove the C runtime dependency only after hardware and interoperability tests pass.
