# NIK LPA documentation index

This directory contains both NIK product documentation and retained upstream `lpac` documentation.

For NIK development, start with the files below.

| Document | Purpose |
|---|---|
| [NIK-LPA-REPOSITORY-GUIDE.md](NIK-LPA-REPOSITORY-GUIDE.md) | complete repository map, ownership and runtime paths |
| [NIK-LPA-ARCHITECTURE.md](NIK-LPA-ARCHITECTURE.md) | software architecture and module responsibilities |
| [NIK-LPA-RSP-RELAY.md](NIK-LPA-RSP-RELAY.md) | detailed staged RSP sequence, functions, input/output fields, certificate roles and validation |
| [NIK-LPA-GLOSSARY.md](NIK-LPA-GLOSSARY.md) | terms, variables, protocol fields and function naming |
| [NIK-LPA-STM32-EUICC.md](NIK-LPA-STM32-EUICC.md) | STM32 Card Agent target, eUICC/APDU HAL, UICC contacts and modem/eUICC hardware topologies |
| [NIK-LPA-WINDOWS-BUILD.md](NIK-LPA-WINDOWS-BUILD.md) | Windows-only CI/build/release policy |

## Reading order for software developers

```text
Repository Guide
 -> Architecture
 -> RSP Relay
 -> Glossary as reference
 -> Windows Build
```

## Reading order for embedded/meter developers

```text
RSP Relay
 -> Glossary
 -> STM32/eUICC integration
```

## Documentation source of truth

NIK-specific documentation describes the behavior of the code on the active NIK development branch. Protocol names and certificate roles use GSMA SGP.22 terminology, but implementation behavior must be verified against the actual `lpac`/`libeuicc` code in this repository.

The inherited project historically targets SGP.22 v2.2.2 behavior. Newer GSMA revisions exist; a compliance/version upgrade should be treated as an explicit implementation and validation project rather than assumed from documentation alone.

## Upstream documents

Other files in this directory may originate from the upstream `lpac` project. They are retained because they remain useful for CLI behavior, development details and compatibility context. Where an upstream document conflicts with a NIK product decision, the NIK-specific documents above are authoritative for the NIK application.
