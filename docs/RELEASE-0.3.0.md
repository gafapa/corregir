# Corregir 0.3.0

This release adds password-protected portable work directories and incorporates the security, persistence, OCR and desktop-workflow improvements documented in the October 9–11 reviews.

## Downloads

- Windows x64 no-install ZIP: extract the complete archive and run `Corregir.exe`. Choose a work directory and create its password, or unlock an existing portable directory. The default relative `work` folder moves with the application. Copy the complete work directory, including `corregir.workspace-key`, only after closing Corregir.
- Windows x64 NSIS installer: retains the installed application's Windows-bound encrypted workspace and credential behavior.
- SHA-256 files accompany the downloads. Both executables are unsigned because no signing certificate is available.

WebView2 must be available on the computer; the portable ZIP does not install it. OCR resources are included. Ollama and its language model are separate local prerequisites. This remains the Phase A synthetic-data prototype; the release is marked as a prerelease.

## Improvements

- Portable work-directory selection and password unlocking, with a random database key protected by Argon2id and AES-256-GCM. Portable opening does not use Windows credentials.
- Database changes are staged in memory and published only after encrypted persistence succeeds; stale assessment revisions and text saves are rejected.
- PDF/image parsing is bounded and runs in a Windows AppContainer with no network capabilities, inherited read-only input and Job Object limits.
- Mixed-PDF OCR, comparison against fingerprint-matched originals, page navigation/zoom and audited text correction before redaction.
- Teacher-reviewed redaction previews, known-name detection and invalidation after text edits.
- Local submission search, status filters and navigation to pending work.
- Bounded local inference queues, cancellation and verified backup-health checks.
- Dependency/Pdfium updates, resource provenance and pinned CI actions, with installer and portable validation workflows.

## Existing workspaces

Installed and 0.2.0 ZIP workspaces are not converted automatically. Export and verify a password-protected backup in the original version, create an empty portable work directory in 0.3.0 and restore that backup. Direct opening of a Windows-bound or legacy plaintext directory in portable mode is refused without replacing its files.

Keep the work-directory password and verified backups separately. A lost password or protected key cannot be reset to decrypt the existing directory; restore a verified backup into a new directory instead. Original source documents are not copied into the work directory. WebView2's runtime/browser profile remains separate.

## Validation and limits

Local validation passed 23 frontend tests, 63 Rust library tests and five real-worker integration tests. The default backend command leaves the shared OS credential-store test and four live-model tests ignored. The four live Ollama checks passed in the earlier review; they were not repeated for the work-directory change.

The normal portable ZIP passed resource hashes, native PDF and mixed OCR checks, and six native-UI phases: creation, wrong-password refusal, reopening, directory relocation, backup restoration into another directory and reopening restored data. The normal personal database fingerprint remained unchanged. The earlier isolated qualification product passed clean installation, upgrade and backup recovery. Those installer checks used a qualification-only picker substitute; they are separate from the actual normal portable ZIP checks.

See [work-directory validation](WORK-DIRECTORY-2026-10-11.md), [desktop qualification](DESKTOP-QUALIFICATION-2026-10-10.md) and [portable instructions](PORTABLE-WINDOWS.md). Dated reports record their validation-time state, including statements about work not yet committed or published at that time. Hosted verification and artifact digests are recorded on the GitHub release when publication completes.

Second-machine provisioning, removable/FAT volumes, network shares, other platforms, real handwriting/institutional qualification and production deployment remain outside the recorded local evidence. Cargo's existing GTK advisories are informational warnings; they are retained in audit output.
