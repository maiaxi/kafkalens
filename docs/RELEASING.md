# Releasing

Releases are built by [`.github/workflows/release.yml`](../.github/workflows/release.yml)
when a `v*` tag is pushed. The workflow produces a universal (Apple Silicon +
Intel) `Kafkalens.app`, zips it, and creates a **draft** GitHub release with
the zip and its SHA-256. Review the draft, then publish it by hand.

## Cutting a release

1. Bump `version` in `core/Cargo.toml` (the app version is read from there).
2. Commit, then tag and push:

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```

   The workflow fails if the tag and the crate version differ.
3. Open the draft on the Releases page, edit the notes, publish.

To build the same artifact locally: `scripts/bundle-app.sh --universal`.

## Signing and notarization

Releases are currently ad-hoc signed and not notarized: macOS blocks the
first launch of each downloaded version, and the release notes and README
explain how to open it anyway.

The workflow can also sign with a Developer ID certificate and notarize with
Apple, which removes that step for users. This needs a paid Apple Developer
Program membership, which the project does not have; the steps below are kept
in case that changes.

| Secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_P12` | The "Developer ID Application" certificate and private key, exported from Keychain Access as `.p12`, base64-encoded (`base64 -i cert.p12 \| pbcopy`). |
| `MACOS_CERTIFICATE_PASSWORD` | Password chosen when exporting the `.p12`. |
| `MACOS_SIGN_IDENTITY` | The identity name, e.g. `Developer ID Application: Jane Doe (TEAMID1234)`. |
| `APPLE_ID` | Apple ID email of the developer account. |
| `APPLE_TEAM_ID` | 10-character team ID. |
| `APPLE_APP_PASSWORD` | An app-specific password for that Apple ID (appleid.apple.com → Sign-In and Security). |

Add them under the repository's Settings → Secrets and variables → Actions.
Signing turns on as soon as `MACOS_CERTIFICATE_P12` is set.
