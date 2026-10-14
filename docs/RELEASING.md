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

## Signing

Releases are signed with a **self-signed** certificate ("Kafkalens Release
Signing") kept in the repository secrets. It does not make macOS trust the
app: the first launch of each downloaded version is still blocked by
Gatekeeper, and the README and release notes explain how to open it. What
it does give is a signature that stays the same across versions, so the
Keychain "Always Allow" for saved passwords survives updates (with ad-hoc
signing every version would ask again).

| Secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_P12` | Certificate and private key as `.p12`, base64-encoded. |
| `MACOS_CERTIFICATE_PASSWORD` | Password of the `.p12`. |
| `MACOS_SIGN_IDENTITY` | Certificate name: `Kafkalens Release Signing`. |

The private key exists only in those secrets. If it is lost or replaced,
create a new certificate (below) and update the three secrets; users then
confirm Keychain access once more after updating.

```sh
cat > cert.cnf <<'CNF'
[req]
distinguished_name = dn
prompt = no
x509_extensions = ext
[dn]
CN = Kafkalens Release Signing
O = Kafkalens
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
CNF
/usr/bin/openssl req -x509 -newkey rsa:3072 -sha256 -days 7300 -nodes \
  -keyout key.pem -out cert.pem -config cert.cnf
/usr/bin/openssl pkcs12 -export -inkey key.pem -in cert.pem \
  -name "Kafkalens Release Signing" -out cert.p12   # prompts for a password
base64 -i cert.p12 | gh secret set MACOS_CERTIFICATE_P12
gh secret set MACOS_CERTIFICATE_PASSWORD             # the password above
gh secret set MACOS_SIGN_IDENTITY --body "Kafkalens Release Signing"
rm key.pem cert.p12
```

Use `/usr/bin/openssl` (LibreSSL): its `.p12` encryption is one the macOS
`security` tool can import.

### Notarization (not used)

Notarization would remove the Gatekeeper step but needs a paid Apple
Developer Program membership, which the project does not have. The workflow
supports it if that changes: replace the certificate secrets with a
"Developer ID Application" certificate and identity, and add:

| Secret | Value |
| --- | --- |
| `APPLE_ID` | Apple ID email of the developer account. |
| `APPLE_TEAM_ID` | 10-character team ID. |
| `APPLE_APP_PASSWORD` | An app-specific password for that Apple ID. |
