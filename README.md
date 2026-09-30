# Corregir

Corregir is a local desktop prototype for teacher-reviewed grading assistance. It imports PDF or image submissions, extracts text locally, requires a teacher to review redaction, and sends only confirmed redacted text to a local Ollama model. The teacher enters and confirms the final grade.

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

1. Create one of the synthetic rubrics and a test assignment.
2. Import a PDF or image submission. PDFs with a text layer use native extraction; scanned pages use local OCR.
3. Review detected identifiers, add any missed fragments, and confirm redaction.
4. Ask the AI to locate evidence, enter your own criterion scores, and request feedback.
5. Confirm the grade, then export the grade CSV, feedback PDF, or audit CSV.

SQLite runs in memory; only authenticated AES-256-GCM snapshots are written to disk. The database credential stays in Windows Credential Manager. Every successful mutation saves an atomic encrypted snapshot, and an exclusive data-directory lock prevents competing application instances. Opening an existing encrypted database without its original credential fails without creating a replacement key.

Existing Spanish schemas and newer changes in a leftover legacy working database are migrated before obsolete plaintext files are removed. Keep the encrypted database and its original credential together when planning recovery; this prototype does not implement a portable backup/credential recovery UI.

Assignments, teacher scores, comments, evidence and feedback survive restarts. Editing scores invalidates feedback; revision checks reject stale saves and AI responses. Imports run on bounded workers with page progress and cancellation after the current page finishes. Limits are 50 MiB, 50 PDF pages, 1 MiB of extracted text, image dimensions up to 6000 pixels per side and 16 million pixels overall. Ollama responses are capped at 2 MiB and 180 seconds per request, with one retry for malformed JSON. Thinking is disabled for supported models, output uses a JSON schema and generation length is bounded. The request context scales between 4096 and 32768 tokens; prompts exceeding a conservative byte budget fail before transmission.

PDF reports wrap and paginate text. Exports cannot overwrite application data or bundled resources. Grade CSV and feedback PDF require confirmed teacher grades; audit CSV contains event metadata and excludes payloads. Grade reports intentionally contain locally resolved names and must be handled accordingly.

## Synthetic fixtures

The `synthetic-data` directory contains fictional rubrics and submissions, including a scanned PDF, an intentionally incorrect physics formula, fictional identifiers, and a prompt injection attempt. To regenerate them, install `reportlab` and `PyMuPDF`, then run:

```sh
python scripts/generate_synthetic_data.py
```

## Documentation

- [Technical architecture](docs/ARCHITECTURE.md)
- [Data protection and fundamental rights impact assessment](docs/DPIA-FRIA.md)
- [Code, security, and dependency review](docs/PROJECT-REVIEW.md)

## Verification

```sh
pnpm verify:resources
pnpm test
pnpm build
pnpm audit
cd src-tauri
cargo fmt --all -- --check
cargo test --lib
cargo audit
```

Install `cargo-audit` with `cargo install cargo-audit --locked` before running the Rust dependency audit. Tests that contact Ollama or the OS credential store are explicitly ignored by default. To run the local inference checks without touching shared credentials, use `cargo test --lib -- --ignored --skip is_idempotent --test-threads=1`. The OCR smoke test runs in the default suite with optimized neural inference kernels.

Windows CI runs resource verification, frontend tests/build/audit and Rust formatting/tests/audit. Dependabot monitors JavaScript, Cargo and workflow dependencies. Bundled resources are pinned with SHA-256 checks enforced by both `pnpm verify:resources` and the Rust build. Their provenance and licenses are recorded in [third-party notices](src-tauri/resources/THIRD-PARTY-NOTICES.md).

The all-platform Rust lockfile still reports two upstream GTK advisories that do not enter the supported Windows build. They remain visible, rather than being suppressed; Linux support is blocked pending native resources and a separate security review. See [project review](docs/PROJECT-REVIEW.md) for current validation and limitations.
