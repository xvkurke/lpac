# NIK LPA glossary

This glossary defines the terms, variables and function naming used by NIK LPA, the patched `lpac` relay applet and the underlying `libeuicc` RSP engine.

## Actors and components

| Term | Meaning | NIK LPA meaning |
|---|---|---|
| LPA | Local Profile Assistant | Software component coordinating consumer eSIM RSP |
| `lpac` | upstream local profile assistant implementation | C executable/protocol engine used by NIK LPA |
| `libeuicc` | eUICC/RSP protocol library | Owns ES9+, ES10b, ASN.1, certificates, APDU and BPP logic |
| NIK LPA | NIK Windows application | Rust UI/orchestration around `lpac`/`libeuicc` |
| eUICC | embedded UICC | Secure element storing eSIM profiles |
| SM-DP+ | Subscription Manager Data Preparation+ | Server that authenticates RSP sessions and prepares/provides profile packages |
| Card Agent | NIK staged relay role | Owns eUICC and ES10b side |
| Server Agent | NIK staged relay role | Owns SM-DP+ and ES9+ side |
| PC/SC | desktop smart-card API | Windows path used to transmit APDUs to a reader/eUICC |
| WinHTTP | Windows HTTP API | HTTP/TLS backend used by Server Agent |

## Protocol interfaces

| Term | Meaning |
|---|---|
| SGP.22 | GSMA Consumer eSIM technical specification |
| ES9+ | interface between LPA and SM-DP+ |
| ES10b | interface used by LPA to execute RSP operations on eUICC |
| APDU | smart-card command/response unit |
| ASN.1 | notation used to define many RSP protocol structures |
| BER-TLV | binary tag/length/value encoding used by smart-card/RSP structures |
| HTTPS/TLS | secure network transport for the SM-DP+ side |
| NDJSON | one JSON object per line; used locally between Rust relay backend and `lpac relay` |
| `NIKRSP-DEBUG1` | current plaintext store-and-forward NIK relay envelope |

## Identifiers

### EID

Identifier of the physical eUICC.

NIK relay does not use the raw EID as its target field. It uses:

```text
target_eid_hash = SHA-256(EID)
```

### ICCID

Identifier of an installed SIM/eSIM profile. After BPP loading NIK LPA obtains an expected ICCID and independently confirms that the same ICCID appears in a fresh Profile List.

### IMSI

Mobile subscriber identity contained in the subscription/profile domain. NIK staged relay does not expose IMSI as an individual profile-install field.

### Ki

SIM authentication secret. NIK LPA does not request or transfer Ki as a standalone plaintext variable. It remains inside the protected profile/eUICC provisioning domain.

### `job_id`

NIK application relay job UUID. It identifies one eight-stage NIK store-and-forward chain.

### `transactionId`

SM-DP+/SGP.22 transaction identifier. It is introduced by `InitiateAuthentication` and must remain unchanged for the rest of the same RSP session.

### `requestId`

Local IPC request correlation UUID used between Rust and the long-lived `lpac relay` process. It identifies one local command, not the full NIK job and not the SM-DP+ RSP transaction.

## Activation data

### Activation Code

Typical shape:

```text
LPA:1$<SM-DP+ address>$<Matching ID>
```

Rust type:

```text
ActivationCode
```

Important fields:

- `raw`: original activation string, held in zeroizing storage;
- `smdp`: SM-DP+ address;
- `matching_id`: Matching ID;
- `confirmation_required`: whether the activation code indicates a confirmation-code requirement.

### `serverAddress`

JSON/C name for the SM-DP+ address. Conceptually equivalent to `ActivationCode.smdp` in the staged flow.

### Matching ID / `matchingId`

Value used by SM-DP+ to match the request to a prepared/pending profile/order. It is not the ICCID.

### Confirmation Code / `confirmationCode`

Optional user/operator secret required by some profiles during PrepareDownload.

## Relay envelope fields

| Field | Meaning |
|---|---|
| `version` | NIK relay packet format version |
| `job_id` | NIK relay UUID |
| `sequence` | strict NIK packet number 1..8 |
| `stage` | current relay stage |
| `direction` | Card→Server or Server→Card |
| `target_eid_hash` | SHA-256 identity binding for the physical eUICC |
| `transaction_id` | SM-DP+ RSP transaction after `SERVER_AUTH` |
| `created_at` | packet creation time |
| `expires_at` | local relay deadline |
| `previous_message_hash` | SHA-256/Base64URL digest of previous NIK packet |
| `payload` | stage-specific RSP/application data |

## Eight NIK relay stages

| # | Stage | Owner |
|---:|---|---|
| 1 | `INIT_REQUEST` | Card Agent |
| 2 | `SERVER_AUTH` | Server Agent |
| 3 | `EUICC_AUTH` | Card Agent |
| 4 | `DOWNLOAD_PREPARE` | Server Agent |
| 5 | `EUICC_PREPARED` | Card Agent |
| 6 | `BOUND_PROFILE_PACKAGE` | Server Agent |
| 7 | `INSTALL_RESULT` | Card Agent |
| 8 | `COMPLETE_ACK` | Server Agent |

`COMPLETE_ACK` is a NIK application-level completion stage; it is not itself an ES10b command.

## RSP variables

### `euiccChallenge`

Fresh challenge returned by eUICC during Card Agent initialization. It is passed to SM-DP+ `InitiateAuthentication` so server authentication material is tied to the active RSP attempt.

### `euiccInfo1`

Encoded `EUICCInfo1` data returned by eUICC. It provides eUICC/RSP capability information needed by the server authentication flow. It is not the EID.

### `serverSigned1`

SM-DP+ signed-data structure used during server authentication. “Signed” does not mean encrypted.

### `serverSignature1`

Digital signature covering the relevant `serverSigned1` material.

### `euiccCiPKIdToBeUsed`

Identifier for the Certificate Issuer public-key context selected for eUICC verification. Naming breakdown: eUICC + CI (Certificate Issuer) + PK (Public Key) + ID.

### `serverCertificate`

Certificate supplied with the first SM-DP+ authentication material and passed to ES10b `AuthenticateServer`. In SGP.22 certificate-role terminology this is the SM-DP+ authentication-signing certificate role commonly referred to as `CERT.DPauth.SIG` in the protocol specification. The NIK relay C adapter intentionally exposes the generic library field name `serverCertificate` rather than parsing certificate internals in Rust.

### `authenticateServerResponse`

Encoded/opaque eUICC response produced by ES10b `AuthenticateServer`. It carries the card-side authentication result/material required by SM-DP+ `AuthenticateClient`.

NIK Rust does not split this response into lower-level certificate/signature fields; it forwards the opaque response unchanged to the Server Agent path.

### `profileMetadata`

Metadata for the profile that SM-DP+ is preparing to deliver. It is not the full profile package.

### `smdpSigned2`

Second SM-DP+ signed-data structure, used for download/profile-binding preparation.

### `smdpSignature2`

Digital signature covering the corresponding `smdpSigned2` data.

### `smdpCertificate`

SM-DP+ certificate used during the second/profile-binding preparation stage. In SGP.22 certificate-role terminology this corresponds to the profile-binding signing certificate role commonly referred to as `CERT.DPpb.SIG`. The NIK C adapter again transports the library-provided encoded certificate rather than parsing it in Rust.

### `prepareDownloadResponse`

Encoded/opaque response produced by eUICC after ES10b `PrepareDownload`. It contains card-generated preparation material needed by SM-DP+ to create/get the BPP. NIK Rust treats it as opaque data.

### BPP / `boundProfilePackage`

Bound Profile Package. Protected profile package created/provided by SM-DP+ for the target eUICC/session. It is not a plain ZIP of IMSI/Ki fields. NIK LPA relays it as protocol data and sends it to `LoadBoundProfilePackage`.

### `sequenceNumber` in LoadBPP result

eUICC/BPP result sequence number. It must not be confused with NIK `RelayPacket.sequence` 1..8.

### `bppCommandId`

Identifier describing the BPP command/result position reported by the eUICC loader.

### `errorReason`

Numeric eUICC/BPP failure reason.

### `errorReasonName`

Human-readable conversion of `errorReason` through `euicc_errorreason2str()`.

## Certificate and trust terms

### CI

Certificate Issuer/root trust context used by the RSP PKI.

### `CERT.DPauth.SIG`

SM-DP+ certificate role used to authenticate/sign the server-authentication part of the RSP flow. In the current relay adapter it is represented through the encoded `serverCertificate` field supplied by `libeuicc`.

### `CERT.DPpb.SIG`

SM-DP+ certificate role used in profile-binding/download preparation. In the current relay adapter it is represented through the encoded `smdpCertificate` field.

### eUICC/EUM certificate material

The eUICC authentication response may internally involve eUICC/EUM certificate and signature structures defined by SGP.22. The current staged NIK relay intentionally does not parse these out in Rust; they remain inside the encoded `authenticateServerResponse` produced by `libeuicc`/eUICC.

## Function naming

### `es9p_*`

Functions implementing the LPA↔SM-DP+ ES9+ side.

Examples:

```text
es9p_initiate_authentication_r
es9p_authenticate_client_r
es9p_get_bound_profile_package_r
es9p_cancel_session_r
```

Think: **network / SM-DP+ side**.

### `es10b_*`

Functions implementing the LPA↔eUICC ES10b side.

Examples:

```text
es10b_get_euicc_challenge_r
es10b_get_euicc_info_r
es10b_authenticate_server_r
es10b_prepare_download_r
es10b_load_bound_profile_package_r
es10b_cancel_session_r
```

Think: **card / eUICC side**.

### `_r`

The current code uses these context-based API variants with an explicit `euicc_ctx` and output parameters. Do not invent a semantic expansion such as “raw” or “result” for the suffix unless upstream explicitly documents it.

### `b64_*`

Base64-encoded protocol data. Base64 is an encoding, not encryption.

### `*_param`

Input parameter structure for a protocol call.

### `*_param_user`

User/LPA-supplied parameters separated from server-provided cryptographic material, for example Matching ID, IMEI or Confirmation Code.

### `*_result`

Output/result structure.

### `*_free()`

Helper that releases dynamic storage held by a protocol parameter/result structure.

## Important C relay helpers

| Function | Purpose |
|---|---|
| `handle_card_init()` | Get eUICC challenge + EUICCInfo1 and expose them as JSON |
| `handle_card_authenticate_server()` | Map `SERVER_AUTH` payload to ES10b AuthenticateServer |
| `handle_card_prepare_download()` | Map `DOWNLOAD_PREPARE` payload to ES10b PrepareDownload |
| `handle_card_install_bpp()` | Pass BPP to ES10b LoadBoundProfilePackage and expose result |
| `handle_card_cancel()` | Cancel active card-side RSP session |
| `handle_server_initiate()` | Execute ES9+ InitiateAuthentication |
| `handle_server_authenticate_client()` | Execute ES9+ AuthenticateClient |
| `handle_server_get_bpp()` | Execute ES9+ GetBoundProfilePackage |
| `handle_server_cancel()` | Cancel active server-side RSP session |
| `run_agent()` | Long-lived NDJSON stdin/stdout dispatcher |
| `secure_clear()` | overwrite sensitive C memory before release |

## Important Rust relay helpers

| Type/function | Purpose |
|---|---|
| `RelayPacket` | NIK transport envelope |
| `RelayPacket::validate_after()` | strict order/identity/hash/transaction validation |
| `RelayPacket::digest()` | SHA-256/Base64URL packet digest |
| `RelaySession` | local state for one side of a staged job |
| `RelaySession::accept_incoming()` | validate then append incoming packet |
| `RelaySession::create_outgoing()` | create the next legal packet |
| `set_outgoing()` | serialize active plaintext `NIKRSP-DEBUG1:` packet |
| `decode_debug_packet()` | deserialize the debug envelope |
| `eid_hash()` | SHA-256 of EID |

## Security vocabulary

### Signed

Integrity/authenticity property created using a private key and checked with the matching trusted public-key/certificate chain.

### Encrypted

Confidentiality property: plaintext is transformed so it cannot be read without the decryption key.

### Encoded

Representation transformation such as Base64 or ASN.1 BER/DER. Encoding alone provides no secrecy.

### Opaque

NIK application deliberately does not interpret the inner structure; it transports the value to the component that owns the protocol semantics.

This distinction is essential when reading the staged flow: many RSP values are Base64-encoded signed or protected structures, while `NIKRSP-DEBUG1` itself is currently plaintext.
