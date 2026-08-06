# NIK LPA desktop application

NIK LPA is a native Windows application built around the existing `lpac`/`libeuicc` protocol engine. The C implementation remains the single owner of PC/SC, APDU, HTTP, ASN.1 processing, certificates, and SGP.22 cryptographic operations. Rust provides the desktop UI, process supervision, encrypted transfer protocol, strict relay state machine, and verification workflow.

## Product modes

The primary executable is `nik-lpa.exe`. One application supports three roles:

- **Local LPA** — PC/SC and Internet are available on one computer. The normal `lpac profile download` path is used.
- **Card Agent** — has the PC/SC reader and eUICC but does not contact the SM-DP+. It executes only staged ES10b operations.
- **Server Agent** — has Internet access and the activation code but does not require a reader. It executes only staged ES9+ operations.

Card Agent and Server Agent may run on two computers or as two application instances. The operator manually copies encrypted `NIKRSP2` strings between the two sessions.

## Real staged RSP flow

The staged path is not a simulation. The bundled C runtime exposes two long-lived helper modes:

```text
lpac relay card-agent
lpac relay server-agent
```

They exchange correlated NDJSON with the Rust application and use the resumable `libeuicc` `_r` APIs.

| Stage | Card Agent / ES10b | Server Agent / ES9+ |
|---|---|---|
| `INIT_REQUEST` | Get eUICC challenge and EUICCInfo1 | InitiateAuthentication |
| `SERVER_AUTH` | AuthenticateServer | Return server-signed material and certificate |
| `EUICC_AUTH` | Return eUICC-signed response | AuthenticateClient |
| `DOWNLOAD_PREPARE` | PrepareDownload | Return profile metadata and SM-DP+ preparation material |
| `EUICC_PREPARED` | Return prepare-download response and one-time key material | GetBoundProfilePackage |
| `BOUND_PROFILE_PACKAGE` | LoadBoundProfilePackage | Return BPP |
| `INSTALL_RESULT` | Run a fresh Profile List and verify ICCID | Receive the verified result |

The Card Agent process stays alive while strings are transferred manually, keeping the PC/SC connection and required eUICC session state open. The Server Agent process similarly remains alive for the active SM-DP+ transaction.

`INSTALL_RESULT` is emitted only after a separate, fresh `profile list` confirms the ICCID returned by `LoadBoundProfilePackage`.

## Encrypted manual transfer

Each application instance generates an X25519 identity and displays a pairing code:

```text
NIKPAIR1:<public-key>
```

After the two instances exchange pairing codes, relay packets use:

```text
NIKRSP2:<authenticated-encrypted-envelope>
```

Cryptography:

- X25519 key agreement;
- HKDF-SHA256 key derivation;
- XChaCha20-Poly1305 authenticated encryption;
- sender and recipient public keys bound as associated data.

A packet is rejected when it is not addressed to the local identity or was not created by the paired peer. The ciphertext does not expose the activation token, confirmation code, certificates, signatures, BPP, or other payload values.

Every relay packet also contains and validates:

- one job UUID;
- strict stage and sequence number;
- direction;
- target EID hash;
- SM-DP+ transaction ID continuity;
- creation time and local expiry;
- previous-message hash.

The GUI displays the protocol timeline and an audit table with timestamp, stage, encrypted packet size, and validation result.

## Components

- `lpac-core` — activation parsing, secret redaction, relay state machine, NIKRSP2 pairing and encryption.
- `lpac-backend` — legacy local-LPA process adapter, reader discovery, secure stdin, NDJSON parsing, and local ICCID verification.
- `lpac-relay-backend` — long-lived Card Agent and Server Agent child-process supervision.
- `lpac-gui` — primary `nik-lpa.exe`, legacy diagnostic GUI, and relay diagnostic tool.
- `fake-lpac` — deterministic process-level backend tests.

The production GUI is split into `app`, `controller`, `logic`, `model`, `views`, `theme`, and reusable `widgets` modules.

## Secret handling

For local download, activation and confirmation codes are sent to the C CLI over stdin rather than argv:

```text
lpac profile download -a -
lpac profile download -a - -c -
```

They therefore do not appear in command-line process listings. Secret GUI fields and temporary Rust/C buffers are cleared where the current types and libraries permit it. Application logs do not print activation or confirmation values.

## Build and test

```powershell
cd rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo build --release -p lpac-gui --bins
```

The full repository CI additionally builds the C runtime across the existing platform matrix and builds the Windows UCRT64 bundle.

## Windows bundle

CI publishes:

```text
nik-lpa-desktop-windows-x86_64
```

Primary files:

- `nik-lpa.exe` — the multi-role application;
- `run-nik-lpa.cmd` — recommended launcher;
- `lpac.exe` — patched C runtime with staged relay applet;
- PC/SC and WinHTTP driver DLLs;
- required UCRT64 runtime libraries.

The bundle also contains the old GUI and Relay Lab as diagnostic executables. They are not the primary product interface.

Extract the complete ZIP into one directory. Do not copy individual EXE files out of the bundle.

## Hardware acceptance test

The staged release candidate is accepted only after this real test:

1. Computer A runs Card Agent with the eUICC in a PC/SC reader and Internet blocked.
2. Computer B runs Server Agent with Internet access and no reader.
3. Both instances exchange `NIKPAIR1` codes once.
4. The operator manually transfers every `NIKRSP2` string.
5. Server Agent communicates with a real SM-DP+.
6. Card Agent loads the returned real BPP.
7. A fresh Profile List confirms the expected ICCID.
8. Card Agent returns the verified `INSTALL_RESULT` to Server Agent.
9. Packet capture on Computer A confirms that Card Agent made no network connection.

## Remaining release work

The current implementation is a hardware-test candidate, not yet a signed final release. Before declaring it production-ready, the project still needs the two-PC hardware acceptance test, encrypted persistent identity/session storage using Windows protection APIs, crash-resume coverage, `.nikrsp` file import/export for large BPP payloads, installer packaging, and code signing.
