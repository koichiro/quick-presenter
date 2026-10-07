# Release SBOM Policy

Quick Presenter publishes a CycloneDX 1.5 JSON Software Bill of Materials
(SBOM) beside each supported release package. This policy implements #265 and
supersedes the v1.0.0 decision to defer SBOM generation. SBOMs complement the
existing advisory audit, packaged license notices, and corresponding source;
they do not replace any of those checks.

## Format and tooling

Use `cargo-cyclonedx` **0.5.9** for the target-specific Cargo normal dependency
graph and `scripts/release_sbom.py` for package inspection and native metadata.
Use `jsonschema` **4.25.1** for schema validation. CycloneDX 1.5 is supported by
the pinned Cargo generator; an SPDX export is not required unless a concrete
consumer needs it. Maintaining one canonical format avoids inconsistent exports.

The Cargo graph uses the release build's target triple and default features,
includes transitive dependencies, and excludes build-only dependencies. It is
resolved dependency metadata, not proof that every listed crate contributes
linked code. If release build features change, update the generator invocation
to match. The generator runs offline after the build, and CI rejects changes to
`Cargo.lock`.

Syft is not required for this first implementation. Package scanning alone does
not provide this project's target/feature-aware Cargo graph or identify all
statically linked PDFium components. `cargo-auditable`, dual-format exports,
SBOM attestations, and automatic vulnerability scanning of SBOMs are separate
future decisions.

## Contents and known limits

Each SBOM identifies:

- Quick Presenter version, full source commit, target triple, and Cargo lockfile
  SHA-256;
- the exact package filename and SHA-256, independently from payload hashes;
- the Cargo dependency graph, with local patched dependencies identified by
  repository path and source commit instead of a pristine registry package URL;
- every extracted regular payload file and its SHA-256, plus symlink targets
  without following links into the host filesystem;
- the PDFium version from each bundled `VERSION`, checked against the pinned
  release in `scripts/pdfium_manifest.json`;
- the PDFium binary distribution URL and verified-at-fetch archive SHA-256,
  separately from the hashes of the actual packaged native libraries;
- PDFium component license text as named **file evidence** records. These
  preserve the upstream license filenames and exact bytes as base64 text
  without claiming that the
  files supply component versions or prove which components are linked;
- MSIX package name, publisher, version, and the fact that it is a submission
  input which Microsoft Store may re-sign.

The SBOM marks its composition as **incomplete**. Upstream PDFium archives
provide component licenses but not a complete versioned dependency inventory.
Rust standard-library components and other statically linked native libraries
are also not fully inventoried. Do not infer versions from license filenames,
claim full native coverage, or mistake absence from an SBOM for absence from a
binary. Review this limitation with every PDFium update; authoritative upstream
component revisions can be added when available.

The existing packaged `licenses/` files and source-offer notice remain mandatory.
An SBOM does not replace them. Corresponding source must contain local patches.

## Artifact coverage and publication

| Channel | SBOM subject and publication |
| --- | --- |
| macOS direct | Final Developer ID signed, notarized, stapled DMG; publish `<dmg filename>.cdx.json` beside the DMG. |
| Ubuntu direct | Final DEB; publish `<deb filename>.cdx.json` beside the DEB. |
| Microsoft Store | Submitted Store-identity MSIX; publish `<msix filename>.cdx.json` with the source release and record the certified Store identity/version. Do not publish the MSIX as a supported direct download. |
| Mac App Store, if shipped | Generate the DMG sidecar for the exact signed app payload used for submission, and record the Store identity/version separately. This is a payload inventory, not a hash of an Apple-distributed package. |
| CI validation | `Build Binaries` generates sidecars for its DMG, DEB, direct MSIX, and Store MSIX when present. They describe those validation/submission inputs, not a later signed release. |

Raw executables and MSI installers are development/validation artifacts and are
not supported release downloads; they do not require separate release SBOMs.
Sidecars live outside packages so generating them does not alter signed content
or introduce a package-hash cycle. The Actions artifact upload includes sidecars,
but maintainers must also attach them to the GitHub release for durable public
access. Do not rely on the expiring Actions artifact alone.

## Generate and verify

Run from the exact release source checkout with Python 3.11 or later. Install
the pinned tools (a Python virtual environment is recommended):

```sh
cargo install cargo-cyclonedx --version 0.5.9 --locked
python -m pip install -r scripts/sbom-requirements.txt
```

After the release build, use the build's target triple (for native builds,
`rustc -vV` reports it as `host`) and matching feature flags:

```sh
CARGO_NET_OFFLINE=true cargo cyclonedx --format json --spec-version 1.5 \
  --target aarch64-apple-darwin --no-build-deps \
  --override-filename quick-presenter-cargo
git diff --exit-code -- Cargo.lock
python scripts/release_sbom.py --cargo-bom quick-presenter-cargo.json \
  --artifact /path/to/QuickPresenter-1.4.0.dmg --target aarch64-apple-darwin \
  --schema-cache /tmp/quick-presenter-sbom-schemas
python scripts/release_sbom.py --cargo-bom quick-presenter-cargo.json \
  --artifact /path/to/QuickPresenter-1.4.0.dmg --target aarch64-apple-darwin \
  --schema-cache /tmp/quick-presenter-sbom-schemas --verify
```

Package extraction uses `hdiutil` on macOS, `dpkg-deb` on Linux, and ZIP extraction
for MSIX. Generate and verify on the corresponding platform. The three official
CycloneDX schema downloads are checksum-pinned in `scripts/sbom_schemas.json`;
validation uses a local cache and resolves schema references without additional
network requests. The first validation needs network access to fill the cache.

`--verify` never rewrites the sidecar. It validates the schema and compares its
entire inventory against freshly extracted package contents and the source
inputs. Missing notices, mismatched versions/targets, unknown local dependencies,
dangling references, or changed package/payload hashes fail the command.

## Release gate

Before publishing:

1. Confirm `Build Binaries` passed SBOM generation and verification for the
   intended release commit and target.
2. After any signing, notarization, stapling, or repackaging, **regenerate and
   verify** the sidecar from the final package. A CI sidecar becomes stale when
   those operations change package or payload bytes.
3. Check the application version, source commit, target, PDFium release/archive,
   local patched dependency source, and the documented native-inventory limits.
4. Record each final package/sidecar pair and the package SHA-256 in the release
   checklist; attach every required sidecar to the GitHub release.
5. For Store channels, record submission identity/version and later certification
   identity/version. Keep submission-input hashes distinct from Store-delivered
   package identity. Publish the Windows submission SBOM when submitting the
   post-release Store update, and record its verification at that time.

Tool or schema updates require a reviewed change to the pinned versions/hashes,
unit tests, and representative package validation on all release platforms.

## Tool references

- [cargo-cyclonedx](https://github.com/CycloneDX/cyclonedx-rust-cargo/tree/main/cargo-cyclonedx)
- [CycloneDX 1.5 schemas](https://github.com/CycloneDX/specification/tree/1.5/schema)
- [Syft package detection](https://oss.anchore.com/docs/capabilities/all-packages/)
