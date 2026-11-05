# MSI signing

Signing keys and certificates must never be stored in this repository. For a
release, obtain an organization-approved Authenticode code-signing certificate
and install the Windows SDK `signtool.exe` on the protected release runner.

1. Store the certificate or signing-service credentials in the organization's
   secret manager, not in GitHub source files or workflow logs.
2. Restrict signing credentials to protected release environments and tag
   workflows; use a hardware-backed or remote signing service when available.
3. Sign the completed MSI with SHA-256 and a trusted timestamp, for example:

   ```powershell
   signtool sign /fd SHA256 /tr https://timestamp.digicert.com /td SHA256 `
     /a target\wix\Atalho-*.msi
   ```

4. Verify the signature before uploading:

   ```powershell
   signtool verify /pa /v target\wix\Atalho-*.msi
   ```

The package workflow currently builds and publishes an unsigned MSI because no
signing certificate has been configured. Configure signing only after a
protected certificate-backed release environment exists.
