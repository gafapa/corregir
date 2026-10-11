# Corregir

[![Verify](https://github.com/gafapa/corregir/actions/workflows/ci.yml/badge.svg)](https://github.com/gafapa/corregir/actions/workflows/ci.yml)

Corregir is a local desktop prototype for teacher-reviewed grading assistance. It imports PDF, text-only DOCX or image submissions, extracts text locally, requires a teacher to review redaction, and sends only confirmed redacted text to a local Ollama model. The teacher enters and confirms the final grade.

This repository implements Phase A with synthetic test data. The architecture and impact assessment describe the controls needed before use with real student data.

## Requirements

- Windows x64 (MSVC); other platforms are not supported by this resource bundle
- Node.js 22.12 or later (Node.js 24 used in CI) and pnpm 10.12.4
- Stable Rust, Microsoft C++ build tools and the Windows Tauri prerequisites
- Ollama running at `http://127.0.0.1:11434` with the `qwen3:8b` model

The desktop bundle includes Pdfium and OCR model resources under `src-tauri/resources`.

## Run

```sh
pnpm install
pnpm tauri dev
```

Debug builds accept an absolute `CORREGIR_DEV_DATA_DIR` environment variable for isolated synthetic development data. Release builds always use the normal application-data directory.

To build the frontend and check the Rust backend:

```sh
pnpm build
cd src-tauri
cargo check
```

## Workflow

1. Create a custom rubric or load a synthetic example, then create a test assignment. Editing a rubric creates a new version; existing assignments and grades retain their original criteria.
2. Import a PDF, text-only DOCX or image submission. Text-only PDF pages use native extraction; pages containing images or form objects use full-page local OCR. Enable “Use OCR on every PDF page” when automatic detection misses visible content. Compare extracted text with the original document. DOCX documents containing images, equations, automatic numbering, fields, tracked changes, headers or notes must be exported as PDF to preserve their content.
3. Open “Compare original and extracted text”, select the original PDF/image and correct recognition mistakes without changing the answer. Its SHA-256 must match the imported source. Save corrections before redaction; changes are audited and the initial extraction remains inside the encrypted workspace. Older submissions without a recorded fingerprint can be compared outside the app.
4. Review detected identifiers, add missed fragments and enter the known full name. Exact full-name matches are redacted automatically. Preview the resulting submission text, check for remaining identifiers, and confirm that reviewed preview. Changes invalidate the preview.
5. Ask the AI to locate evidence, enter your own criterion scores, and request feedback.
6. Confirm the grade, then export the grade CSV, feedback PDF, or audit CSV.

SQLite runs in memory; only authenticated AES-256-GCM snapshots are written to disk. The database credential stays in Windows Credential Manager. Each mutation uses a private staged database and becomes visible only after its atomic encrypted snapshot is saved. Failed writes leave both the active database and previous snapshot unchanged, and an exclusive data-directory lock prevents competing application instances. Opening an existing encrypted database without its original credential fails without creating a replacement key.

Existing Spanish schemas and newer changes in a leftover legacy working database are migrated before obsolete plaintext files are removed. The updated encryption library retains compatibility with existing AES-GCM snapshots; a regression test verifies interoperability with the previous library.

Diagnostics saves the local Ollama URL and model in the encrypted workspace. Only local HTTP endpoints are accepted. Its encrypted-backup tools export password-protected `.corregirbackup` snapshots using Argon2id (64 MiB, three iterations) and authenticated AES-256-GCM. Use a unique password of at least 12 characters and keep it separately from the backup. Ordinary restore requires an empty workspace and never replaces existing records. Backups are limited to 128 MiB. Startup and daily reminders flag missing, changed or week-old copies. “Check recovery without restoring” authenticates and validates a selected backup without replacing the current workspace; its password is not retained.

If the original device credential is unavailable or cannot decrypt the database, startup offers recovery from a previously exported backup. Recovery preserves the original encrypted file as `corregir-before-recovery-*.enc`, then seals the restored workspace with the current device credential. It restores only records included in the backup. Without the original credential or a usable backup and password, encrypted records cannot be recovered. Recovery refuses leftover legacy plaintext working files rather than discarding them.

Search submissions locally, filter by workflow state and use “Next pending submission” to move through unfinished work. Unsaved OCR corrections block filtering and tab navigation until saved or discarded.

Assignments, teacher scores, comments, evidence and feedback survive restarts. Editing scores invalidates feedback; revision checks reject stale saves and AI responses. Imports run in a supervised child process with page progress; cancellation terminates the worker even during native parsing or OCR. On Windows, a Job Object limits the worker to one process and 1 GiB of committed memory, and terminates it when its supervising job closes. The worker has no workspace or credential-store initialization. On Windows it runs in an AppContainer with no network capabilities, an explicit inherited-handle list and a read-only handle to the selected document. Only public executable/model resources receive temporary read grants; original file permissions stay unchanged. Standard Windows path variables are allowlisted, without PATH or credentials. Sandbox launch failures stop processing; there is no unrestricted fallback. Limits are 50 MiB, 50 PDF pages, 1 MiB of extracted text, image dimensions up to 6000 pixels per side and 16 million pixels overall. Ollama requests share one execution slot and at most three waiting slots, with queue/running/saving progress and cancellation before saving. Cancellation aborts the client request; the model server may continue generation internally. Ollama responses are capped at 2 MiB and 180 seconds per request, with one retry for malformed JSON. Thinking is disabled for supported models, output uses a JSON schema and generation length is bounded. The request context scales between 4096 and 32768 tokens; prompts exceeding a conservative byte budget fail before transmission.

PDF reports wrap and paginate text. Exports cannot overwrite application data or bundled resources. Grade CSV and feedback PDF require confirmed teacher grades; audit CSV contains event metadata and excludes payloads. Grade reports intentionally contain locally resolved names and must be handled accordingly.

## Synthetic fixtures

The `synthetic-data` directory contains fictional rubrics and submissions, including scanned and mixed text/image PDFs, an intentionally incorrect physics formula, fictional identifiers, and a prompt injection attempt. To regenerate them, install `reportlab` and `PyMuPDF`, then run:

```sh
python scripts/generate_synthetic_data.py
```

## Windows installer

The manually dispatched [Windows installer workflow](https://github.com/gafapa/corregir/actions/workflows/package.yml) verifies resources, runs frontend/backend tests, builds the release-profile NSIS installer, installs it silently, verifies installed resource hashes and smoke-tests document extraction in the installed executable and two application starts against the same encrypted workspace. It uploads the installer as an Actions artifact for 14 days. The installer is unsigned: trusted public distribution still requires a code-signing certificate and an authenticated signing process. No certificate or signing credentials are included in this repository.

## No-install Windows package

Release notes and migration guidance: [Corregir 0.3.0](docs/RELEASE-0.3.0.md). Published downloads are available on the [GitHub release page](https://github.com/gafapa/corregir/releases/tag/v0.3.0).

A [no-install Windows ZIP](docs/PORTABLE-WINDOWS.md) can be generated with `pnpm package:portable`. Extract the entire archive and launch `Corregir.exe`. Version 0.3.0 opens a selectable, password-protected work directory, defaulting to `work` beside the executable. Its database and protected key move together without Windows credentials. Existing installed/0.2.0 workspaces require verified backup export and restoration into a new portable directory; they are not converted automatically. WebView2 must already be available and its browser profile remains in Windows application data; Ollama/model installation is separate.

## Installed desktop qualification

```sh
pnpm qualify:windows
```

This Windows-only script builds and installs separate qualification versions 0.1.1 and 0.1.2, checks installed hashes, exercises the real native IPC/UI, verifies records after restart and upgrade, then restores a password-protected backup into a fresh qualification workspace and reopens it. It requires a clean `com.corregir.qualification` profile and uses a separate Windows credential entry. On success it archives both encrypted test workspaces and uninstalls the qualification application; its separate credential remains available for later runs. Each run's evidence stays in a unique directory under `output/desktop-qualification`; the normal workspace and credential are untouched. Only file picker selection is substituted. A deterministic localhost Ollama-compatible test server exercises the HTTP boundary; it does not validate model quality. The optional [qualification workflow](.github/workflows/qualify.yml) runs the same script on manual dispatch.

To sign a production build, install a valid code-signing certificate with its private key and configure Tauri's Windows `certificateThumbprint` and timestamp settings. [sign-installer.ps1](scripts/sign-installer.ps1) can sign and verify an existing installer with a selected certificate. No private keys or passwords belong in this repository; signing cannot be completed without the certificate.

## Documentation

- [Technical architecture](docs/ARCHITECTURE.md)
- [Data protection and fundamental rights impact assessment](docs/DPIA-FRIA.md)
- [Code, security, and dependency review](docs/PROJECT-REVIEW.md)
- [Project review (2026-10-09)](docs/REVIEW-2026-10-09.md)
- [Implemented improvements and validation (2026-10-10)](docs/IMPROVEMENTS-2026-10-10.md)
- [Desktop qualification and OCR review (2026-10-10)](docs/DESKTOP-QUALIFICATION-2026-10-10.md)
- [Windows portable package validation (2026-10-10)](docs/PORTABLE-VALIDATION-2026-10-10.md)
- [Password-protected work directories (2026-10-11)](docs/WORK-DIRECTORY-2026-10-11.md)

## Verification

```sh
pnpm verify:resources
pnpm test
pnpm build
pnpm audit
cd src-tauri
cargo fmt --all -- --check
cargo test --locked
cargo audit
```

Install `cargo-audit` with `cargo install cargo-audit --locked` before running the Rust dependency audit. Tests that contact Ollama or the OS credential store are explicitly ignored by default. To run the local inference checks without touching shared credentials, use `cargo test --lib -- --ignored --skip is_idempotent --test-threads=1`. The OCR smoke test runs in the default suite with optimized neural inference kernels.

[Windows CI on GitHub Actions](https://github.com/gafapa/corregir/actions/workflows/ci.yml) runs resource verification, frontend tests/build/audit and Rust formatting/tests/audit on pushes, pull requests, manual dispatch and a weekly schedule. It does not require a local development machine or an Ollama installation. Dependabot monitors JavaScript, Cargo and workflow dependencies; repository security alerts and automatic security updates are enabled. Bundled resources are pinned with SHA-256 checks enforced by both `pnpm verify:resources` and the Rust build. Their provenance and licenses are recorded in [third-party notices](src-tauri/resources/THIRD-PARTY-NOTICES.md).

The all-platform Rust lockfile still reports two upstream GTK advisories that do not enter the supported Windows build. They remain visible, rather than being suppressed; Linux support is blocked pending native resources and a separate security review. See [project review](docs/PROJECT-REVIEW.md) for current validation and limitations.
