# Corregir

[![Verify](https://github.com/gafapa/corregir/actions/workflows/ci.yml/badge.svg)](https://github.com/gafapa/corregir/actions/workflows/ci.yml)

Corregir is a local desktop prototype for teacher-reviewed grading assistance. It imports PDF, text-only DOCX or image submissions, extracts text locally, requires a teacher to review redaction, and sends only confirmed redacted text to a local Ollama model. The teacher enters and confirms the final grade, and edits the feedback before it reaches a report.

The interface, reports and grade spreadsheets are available in Galician, Spanish and English. A teacher guide in Galician is in [docs/GUIA-PROFESORADO.md](docs/GUIA-PROFESORADO.md).

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

1. Optionally save each group's roster under **Classes** (one name per line, pasted from a spreadsheet). Rosters stay in the encrypted workspace.
2. Create a custom rubric, optionally with performance levels per criterion, or load a synthetic example. Editing a rubric creates a new version; existing assignments and grades retain their original criteria.
3. Create an assignment with a title, its rubric and, optionally, its class.
4. Import one or more PDF, text-only DOCX or image submissions in one selection; they are processed sequentially. Text-only PDF pages use native extraction; pages containing images or form objects use full-page local OCR. Enable “Use OCR on every PDF page” when automatic detection misses visible content. DOCX documents containing images, equations, automatic numbering, fields, tracked changes, headers or notes must be exported as PDF to preserve their content.
5. Select a submission in the list. Open “Compare original and extracted text”, select the original PDF/image and correct recognition mistakes without changing the answer. Its SHA-256 must match the imported source. Save corrections before redaction; changes are audited and the initial extraction remains inside the encrypted workspace.
6. Redaction runs identifier detection automatically and suggests the student from the file name and detected roster names. Choose the student, review detected identifiers, add missed fragments, preview the resulting text and confirm that reviewed preview. Exact full-name matches are redacted automatically.
7. Ask the AI to locate evidence (for one submission or all ready ones), enter your scores (clicking a performance level fills its score) and comments, and request a feedback draft. The draft appears in an editable field; reports contain only the text you save.
8. Confirm the grade, then export the grade spreadsheet, one feedback PDF or every feedback PDF of the assignment into a folder. A confirmed grade can be reopened with an audited reason.
9. Under **Privacy**, set the retention period agreed with the DPO, purge identities from finished assignments, and export or erase one student's data.

SQLite runs in memory; only authenticated AES-256-GCM snapshots are written to disk. The database credential stays in Windows Credential Manager. Each mutation uses a private staged database and becomes visible only after its atomic encrypted snapshot is saved. Failed writes leave both the active database and previous snapshot unchanged, and an exclusive data-directory lock prevents competing application instances. Opening an existing encrypted database without its original credential fails without creating a replacement key.

Existing Spanish schemas and newer changes in a leftover legacy working database are migrated before obsolete plaintext files are removed. The updated encryption library retains compatibility with existing AES-GCM snapshots; a regression test verifies interoperability with the previous library.

Diagnostics saves the local Ollama URL and model in the encrypted workspace. Only local HTTP endpoints are accepted. Its encrypted-backup tools export password-protected `.corregirbackup` snapshots using Argon2id (64 MiB, three iterations) and authenticated AES-256-GCM. Use a unique password of at least 12 characters and keep it separately from the backup. Ordinary restore requires an empty workspace and never replaces existing records. Backups are limited to 128 MiB. Startup and daily reminders flag missing, changed or week-old copies. “Check recovery without restoring” authenticates and validates a selected backup without replacing the current workspace; its password is not retained.

If the original device credential is unavailable or cannot decrypt the database, startup offers recovery from a previously exported backup. Recovery preserves the original encrypted file as `corregir-before-recovery-*.enc`, then seals the restored workspace with the current device credential. It restores only records included in the backup. Without the original credential or a usable backup and password, encrypted records cannot be recovered. Recovery refuses leftover legacy plaintext working files rather than discarding them.

Search submissions locally by name, file name, number or text, filter by workflow state and use “Next pending submission” to move through unfinished work. Unsaved OCR corrections block filtering and tab navigation until saved or discarded.

Evidence quotes are matched after folding whitespace, hyphenated line breaks, typographic quotes and letter case, and the stored quote is the exact excerpt of the redacted text. Quotes the model returns that do not occur in the text are discarded and counted rather than failing the request. Feedback prompts include the rubric's grade level, subject and performance levels and the teacher's comments. In those comments the student's full name and its parts are replaced by the alias, and the names of the assignment's other students and of its class roster are replaced by `[…]` (full names in any case; single name parts only when capitalized, because many surnames are also ordinary words).

Submissions, assignments, unused rubric versions and classes can be deleted after a confirmation dialog. Purging an assignment's identities deletes the alias-to-name links, unredacted extractions and source file names of its confirmed submissions and replaces known names in comments and feedback; scores, redacted text and feedback remain. Per-student exports (JSON) and erasure cover submissions and roster entries; erasure also removes the student's name from other students' comments and feedback. Encrypted backups created earlier still contain deleted records until they are destroyed. All of these actions are audited without names.

Assignments, teacher scores, comments, evidence and feedback survive restarts. Editing scores invalidates feedback; revision checks reject stale saves and AI responses. Imports run in a supervised child process with page progress; cancellation terminates the worker even during native parsing or OCR. On Windows, a Job Object limits the worker to one process and 1 GiB of committed memory, and terminates it when its supervising job closes. The worker has no workspace or credential-store initialization. On Windows it runs in an AppContainer with no network capabilities, an explicit inherited-handle list and a read-only handle to the selected document. Only public executable/model resources receive temporary read grants; original file permissions stay unchanged. Standard Windows path variables are allowlisted, without PATH or credentials. Sandbox launch failures stop processing; there is no unrestricted fallback. Limits are 50 MiB, 50 PDF pages, 1 MiB of extracted text, image dimensions up to 6000 pixels per side and 16 million pixels overall. Ollama requests share one execution slot and at most three waiting slots, with queue/running/saving progress and cancellation before saving. Cancellation aborts the client request; the model server may continue generation internally. Ollama responses are capped at 2 MiB and 180 seconds per request, with one retry for malformed JSON. Thinking is disabled for supported models, output uses a JSON schema and generation length is bounded. The request context scales between 4096 and 32768 tokens; prompts whose conservative token estimate exceeds the context fail before transmission.

PDF reports include the student, assignment, subject, date, each criterion with its score and achieved level, teacher comments, the total, a grade out of 10 and the teacher-saved feedback, noting when it was drafted with AI assistance. They wrap and paginate text. Grade spreadsheets have one column per criterion plus total, maximum and grade out of 10, sorted by student; Galician and Spanish exports use semicolons and decimal commas with a UTF-8 byte-order mark. Batch PDF export never replaces existing files. Exports cannot overwrite application data or bundled resources. Grade CSV and feedback PDF require confirmed teacher grades and are audited; audit CSV contains event metadata and excludes payloads. Grade reports intentionally contain locally resolved names and must be handled accordingly.

## Synthetic fixtures

The `synthetic-data` directory contains fictional rubrics and submissions, including scanned and mixed text/image PDFs, an intentionally incorrect physics formula, fictional identifiers, and a prompt injection attempt. To regenerate them, install `reportlab` and `PyMuPDF`, then run:

```sh
python scripts/generate_synthetic_data.py
```

## Windows installer

The manually dispatched [Windows installer workflow](https://github.com/gafapa/corregir/actions/workflows/package.yml) verifies resources, runs frontend/backend tests, builds the release-profile NSIS installer, installs it silently, verifies installed resource hashes and smoke-tests document extraction in the installed executable and two application starts against the same encrypted workspace. It uploads the installer as an Actions artifact for 14 days. The installed executable is `corregir.exe`; installers up to 0.3.0 used `tauri-app.exe`, which an upgrade removes while redirecting its shortcuts. The installer is unsigned: trusted public distribution still requires a code-signing certificate and an authenticated signing process. No certificate or signing credentials are included in this repository.

## No-install Windows package

Release notes and upgrade guidance: [Corregir 0.4.0](docs/RELEASE-0.4.0.md) and [Corregir 0.3.0](docs/RELEASE-0.3.0.md). Published downloads are on the [GitHub releases page](https://github.com/gafapa/corregir/releases).

A [no-install Windows ZIP](docs/PORTABLE-WINDOWS.md) can be generated with `pnpm package:portable`. Extract the entire archive and launch `Corregir.exe`. Since version 0.3.0 it opens a selectable, password-protected work directory, defaulting to `work` beside the executable. Its database and protected key move together without Windows credentials. Existing installed/0.2.0 workspaces require verified backup export and restoration into a new portable directory; they are not converted automatically. WebView2 must already be available and its browser profile remains in Windows application data; Ollama/model installation is separate.

## Installed desktop qualification

```sh
pnpm qualify:windows
```

This Windows-only script builds and installs separate qualification versions 0.1.1 and 0.1.2, checks installed hashes, exercises the real native IPC/UI, verifies records after restart and upgrade, then restores a password-protected backup into a fresh qualification workspace and reopens it. It requires a clean `com.corregir.qualification` profile and uses a separate Windows credential entry. On success it archives both encrypted test workspaces and uninstalls the qualification application; its separate credential remains available for later runs. Each run's evidence stays in a unique directory under `output/desktop-qualification`; the normal workspace and credential are untouched. The first run covers a class roster, a rubric with performance levels, roster suggestion during redaction, level scoring, teacher comments with names hidden from the model, edited feedback, reopening, CSV/single/batch PDF exports, cancelled and confirmed deletion, a per-student access export and the Galician interface. Only file picker selection and confirmation dialogs are substituted. A deterministic localhost Ollama-compatible test server exercises the HTTP boundary; it does not validate model quality. The optional [qualification workflow](.github/workflows/qualify.yml) runs the same script on manual dispatch.

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
- [Classroom workflow, languages and data lifecycle (2026-10-11)](docs/IMPROVEMENTS-2026-10-11.md)
- [Teacher guide (Galician)](docs/GUIA-PROFESORADO.md)
- [Changelog](CHANGELOG.md)

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
