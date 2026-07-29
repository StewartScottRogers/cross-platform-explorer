# Code signing

## Windows — self-signed (CPE-1131)

Release installers built in CI are Authenticode-signed with a **self-signed** certificate
(`cpe-selfsign.cer` in this folder is its **public** half; the private `.pfx` lives only in the repo
secrets `WINDOWS_CERT_PFX_BASE64` + `WINDOWS_CERT_PASSWORD`, never committed).

- **Subject:** `CN=Cross-Platform Explorer, O=Stewart Rogers`
- **Thumbprint:** `060970800049A0AD3614AD235643DA1BF7F2795B`
- **Key:** RSA-3072 / SHA-256, Code Signing EKU, valid 5 years from 2026-07-29.

### What self-signing does and does NOT do

- ✅ The installer/app is genuinely Authenticode-signed and timestamped — no "this file has no signature"
  state, and tamper-evidence.
- ✅ On any machine that **trusts this certificate** (below), Windows shows the real publisher
  ("Cross-Platform Explorer") instead of "Unknown publisher".
- ❌ It does **NOT** clear Microsoft SmartScreen / "unknown publisher" for the general public. Only a
  CA-issued **OV/EV** Authenticode certificate does that (tracked in **CPE-002**). Use self-signing for
  your own machines / a controlled fleet and for validating the signing pipeline.

### Trust the certificate (per machine, one-time)

Run PowerShell **as Administrator** from this folder:

```powershell
# Trust it as a root + as a trusted software publisher
Import-Certificate -FilePath .\cpe-selfsign.cer -CertStoreLocation Cert:\LocalMachine\Root
Import-Certificate -FilePath .\cpe-selfsign.cer -CertStoreLocation Cert:\LocalMachine\TrustedPublisher
```

After that, `Get-AuthenticodeSignature <the-installer>.exe` reports `Valid` and the UAC/publisher prompts
show the real name. To undo, delete the cert from those two stores (`certlm.msc`).

### Rotating / regenerating the cert

The cert was generated with `New-SelfSignedCertificate -Type CodeSigningCert`. To rotate: generate a new
one, `Export-PfxCertificate`, base64 the `.pfx`, and update the two repo secrets — CI derives the thumbprint
from the imported `.pfx` at build time, so no code change is needed. Replace this folder's public `.cer` and
its thumbprint above.

## macOS

Not self-signable meaningfully — Gatekeeper requires an Apple **Developer ID** certificate + notarization.
Tracked in **CPE-002**.
