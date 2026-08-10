# NIK LPA Windows build and release pipeline

## Product build policy

NIK LPA is currently a Windows-only product.

The repository may retain portable upstream source code, but CI and product artifacts are intentionally limited to Windows. Linux, Linux ARM, QMI-specific Linux builds, Windows ARM and macOS are not part of the NIK product build matrix.

## CI workflows

### `.github/workflows/build.yaml`

Purpose: build the patched native C `lpac` runtime for Windows x86_64.

Current target:

```text
Windows x86_64 with MinGW
```

The workflow runs on Ubuntu only as a cross-build host. The produced binary target is Windows.

High-level steps:

```text
checkout
 -> setup Debian MinGW toolchain
 -> .github/scripts/build-ci.sh mingw
 -> upload lpac Windows x64 artifact
```

No Linux or macOS product artifact is built by this workflow.

### `.github/workflows/rust-gui.yml`

Purpose: validate and assemble the complete Windows desktop bundle.

Main stages:

```text
checkout
 -> install Rust stable + rustfmt + clippy
 -> setup MSYS2 UCRT64 native Windows toolchain
 -> cargo fmt
 -> cargo clippy -D warnings
 -> cargo build workspace/all targets
 -> cargo test workspace
 -> cargo build --release -p lpac-gui --bins
 -> native CMake/Ninja build of lpac under UCRT64
 -> install native runtime stage
 -> assemble desktop-bundle
 -> isolated bundle smoke test
 -> upload nik-lpa-desktop-windows-x86_64
```

## Primary product artifact

```text
nik-lpa-desktop-windows-x86_64
```

The bundle contains the product executable and the native runtime components it depends on.

Expected core files include:

```text
nik-lpa.exe
lpac.exe
libgcc_s_seh-1.dll
libwinpthread-1.dll

driver/driver_apdu_pcsc.dll
driver/driver_apdu_stdio.dll
driver/driver_http_stdio.dll
driver/driver_http_winhttp.dll
```

The engineering bundle may additionally contain diagnostic/legacy development binaries such as the relay lab executable. Those are not the primary end-user entry point.

## Runtime roles

### Local LPA

```text
LPAC_APDU=pcsc
LPAC_HTTP=winhttp
```

The same machine owns the reader and Internet connection.

### Card Agent

```text
LPAC_APDU=pcsc
LPAC_HTTP=stdio
```

The process owns the physical eUICC and does not use the normal Internet HTTP backend.

### Server Agent

```text
LPAC_APDU=stdio
LPAC_HTTP=winhttp
```

The process owns Internet/SM-DP+ access and does not own a PC/SC reader.

## Local Rust validation

On a Windows developer machine with the Rust toolchain installed:

```powershell
cd rust
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --all-targets
cargo test --workspace
cargo build --release -p lpac-gui --bins
```

These commands validate the Rust workspace. The complete production bundle still depends on the native Windows `lpac` runtime and its driver DLLs, which CI assembles automatically.

## Native C build notes

The Windows desktop CI builds the native runtime with CMake/Ninja under MSYS2 UCRT64 and disables the curl HTTP backend for that bundle:

```text
-DSTANDALONE_MODE=ON
-DCMAKE_BUILD_TYPE=Release
-DCMAKE_INTERPROCEDURAL_OPTIMIZATION=OFF
-DLPAC_WITH_HTTP_CURL=OFF
```

The Windows product uses the WinHTTP driver for real SM-DP+ network access.

## Bundle smoke test

The CI smoke test intentionally changes the runtime environment to verify that the bundle is self-contained enough to launch the patched `lpac.exe` without relying on an accidentally inherited developer PATH.

It verifies required EXEs/DLLs and executes:

```text
lpac.exe version
```

with stdio APDU/HTTP backends in the isolated bundle directory.

## Release rule

A product release should be considered valid only when the Windows workflows are green for the exact commit being released.

Required validation categories:

- C/native Windows runtime build;
- Rust formatting;
- Clippy with warnings denied;
- Rust workspace build;
- Rust tests;
- native UCRT64 build;
- Windows bundle assembly;
- isolated bundle smoke test.

## Why portable source is not deleted

Windows-only product support does not imply that every upstream `#ifdef`, CMake portability branch or portable library file must be removed.

Deleting portable upstream code would create unnecessary divergence from `lpac`/`libeuicc` and make future upstream integration harder. The current policy is therefore:

```text
product/CI targets = Windows only
source portability = retained unless it directly harms the Windows product
```
