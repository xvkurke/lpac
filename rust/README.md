# NIK LPA Rust desktop layer

This directory contains the Rust desktop application and orchestration layer built around the existing `lpac`/`libeuicc` engine.

The C implementation remains the single owner of the eUICC, PC/SC connection, APDU transport, and SGP.22 protocol state. The Rust side does not implement a second smart-card or RSP stack.

## Components

- `lpac-core`: strict activation-code parsing, redacted secret types, encrypted activation jobs, and `NIKLPA1` transfer envelopes.
- `lpac-backend`: typed process adapter for the C `lpac` executable, NDJSON progress parsing, secure stdin input, reader discovery, and post-install verification.
- `lpac-gui`: native egui/eframe application for PC/SC laboratory use.
- `fake-lpac`: deterministic process-level test harness for backend and secret-boundary tests.

## Supported desktop workflow

The GUI currently provides:

- explicit PC/SC reader discovery through `lpac driver apdu list`;
- manual reader-index fallback;
- eUICC information lookup;
- profile listing;
- local profile installation from an activation string;
- encrypted export of an activation job;
- import and decryption of an activation job on another device;
- one card operation at a time, executed outside the UI thread;
- structured progress and final-result logging without activation secrets;
- mandatory `profile list` verification after download.

An installation is reported as successful only when:

1. `lpac profile download` returns a successful final event containing a non-empty ICCID; and
2. a fresh `lpac profile list` contains the same ICCID.

## Secure stdin contract

The Rust backend invokes the patched C CLI as:

```text
lpac profile download -a -
```

or, when a confirmation code is present:

```text
lpac profile download -a - -c -
```

The activation and confirmation codes are written as separate lines to stdin. They do not appear in argv, process listings, or application logs. The C buffers and Rust temporary buffers are cleared after use.

## Encrypted transfer string

The laboratory transfer format is:

```text
NIKLPA1:<base64url authenticated envelope>
```

The payload is encrypted and authenticated with XChaCha20-Poly1305. It contains:

- the exact activation string;
- an optional confirmation code;
- optional reader and EID hints;
- a job UUID and creation timestamp.

The sender copies the `NIKLPA1` string and delivers the 256-bit transfer key through a separate trusted channel. The receiver pastes both values into the GUI, decrypts the job, reviews only redacted metadata, selects its local reader, and installs the job.

After a successful import, the entered key and ciphertext fields are cleared. The decrypted job zeroizes its activation and confirmation strings when dropped.

> The shared transfer key is intended for the current laboratory test. Production enrollment should use recipient public-key encryption or a device-bound key held by TPM, secure element, or another protected keystore.

## Build and test

```bash
cd rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo run -p lpac-gui
```

For a local development build, place the patched `lpac` executable next to the GUI, keep its driver/runtime layout intact, or select its path in the application.

## Desktop bundles

CI publishes:

- `nik-lpa-desktop-windows-x86_64`
- `nik-lpa-desktop-linux-x86_64`

Each bundle contains:

- the Rust GUI;
- the patched `lpac` executable from the same head SHA;
- PC/SC and HTTP driver libraries;
- the C runtime libraries and license files;
- a platform launcher that starts the GUI from the bundle directory.

Use `run-nik-lpa.cmd` on Windows or `run-nik-lpa.sh` on Linux.

System requirements remain external:

- Windows Smart Card service must be running;
- Linux requires `pcscd` and the normal desktop/OpenGL runtime dependencies;
- a compatible PC/SC reader and removable eUICC must be connected for hardware operations.

The packaging workflow smoke-tests the bundled `lpac` runtime with `lpac version` and `lpac driver list` before upload. Hardware access is not exercised in hosted CI.

## Integration coverage

The `fake-lpac` harness verifies that:

- PC/SC readers are discovered through `lpac` without opening a second card connection;
- activation and confirmation codes arrive through stdin;
- neither secret is present in argv;
- progress and final NDJSON events are parsed correctly;
- structured PC/SC errors propagate to the GUI layer;
- a successful download is followed by ICCID verification through `profile list`;
- formatted logs do not contain the activation or confirmation code.

## Roadmap

1. Run a hardware smoke test against the target Windows PC/SC reader and removable eUICC.
2. Add richer profile-management screens, cancellation, and explicit confirmation dialogs.
3. Replace the laboratory shared-key envelope with recipient public-key encryption and device enrollment.
4. Add replay fixtures for sanitized real `lpac` event streams.
5. Keep the protocol/APDU implementation in `libeuicc` until an independently tested Rust replacement provides a concrete maintenance or safety benefit.
