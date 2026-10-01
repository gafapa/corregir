# Project review and remediation

Date: 2026-09-30. Scope: the current working tree, including the existing English renames and legacy schema migration. Existing user changes were preserved. The implemented and tested distribution target is Windows x64 MSVC.

## Resolved findings

| Area | Resolution |
| --- | --- |
| Plaintext database and crash recovery | SQLite runs in memory. AES-256-GCM snapshots are atomically replaced and synchronized after successful mutations. Existing snapshots are authenticated before legacy working copies are recovered and migrated. Only obsolete application-owned plaintext files are then removed. Wrong keys never overwrite the snapshot. |
| Simultaneous instances and credential creation | Acquire an exclusive data-directory lock before accessing the credential store or snapshots. Refuse to create a replacement credential for an existing encrypted database. |
| Database integrity | Enable foreign keys, secure deletion, memory-backed temporary storage and transactions for multi-write operations. Close and final persistence share one lock. Secure close runs on a worker and keeps the window open with an error when saving fails. |
| Unicode redaction and reviewed workflow | Convert browser UTF-16 positions to UTF-8 byte offsets, reject invalid boundaries, merge overlapping spans and reject repeated confirmation. No AI request accepts unreviewed student text. |
| Resource exhaustion | Limit files to 50 MiB, PDFs to 50 pages, extracted text to 1 MiB, images to 6000 pixels per dimension and 16 million pixels overall, with a 64 MiB decoder allocation limit. Consume PDF pages individually. Bound Ollama responses to 2 MiB, connection establishment to 5 seconds and each request to 180 seconds. |
| Desktop responsiveness | Use bounded workers for database/import/export operations. Serialize imports; report progress and allow cancellation between pages. Reuse Pdfium's process-wide bindings instead of initializing them for every document. |
| Persisted workflow and rubric selection | List and reopen assignments; restore scores, comments, evidence and current feedback. The active assignment retains its actual rubric when preparing another assignment. |
| Stale scores and feedback | Save all scores transactionally with a revision precondition. Editing invalidates feedback, and late AI responses are rejected if the score revision changed. Final confirmation requires reviewed text and a valid teacher score for every criterion. |
| Inference boundary and output validation | Restrict inference URLs to local HTTP without credentials, query, fragment or additional path. Disable proxies and redirects. Reject malformed/unknown output fields, incomplete or duplicate criteria and non-verbatim evidence. Disable supported-model thinking, require schema-constrained output, adapt context size between 4096 and 32768 tokens, reject oversized prompts before transmission, and bound generation length with deterministic sampling. |
| Long PDF reports | Replace the PDF/font dependency chain with maintained `lopdf` and `skrifa`. Embed Roboto with Unicode mappings, wrap words using font metrics, paginate all fields and add page numbers. Export only confirmed grades and feedback for the matching score revision. |
| Export protection | Require absolute CSV/PDF destinations, canonicalize directories, reject application data/resource paths, and replace files atomically. Escape spreadsheet formula prefixes. Audit CSV excludes payload bodies and includes event metadata only. |
| Frontend robustness and accessibility | Catch file-dialog/reload errors, fix deferred access to cleared React event targets, expose errors/status/progress to assistive technology, add input labels, visible focus, larger controls and responsive styling. Remove starter logos/favicon and the unused opener plugin/capability. |
| Supply chain and regression checks | Add frontend regression tests, Windows CI, scheduled audits and Dependabot. Pin bundled resources with verified hashes, source revisions, licenses and notices. Rust builds and CI reject changed resource bytes. |

## Dependency and resource updates

Applied updates include Tauri 2.12.0 and its current runtime dependency chain, dialog 2.8.0, TypeScript 7.0.2, Reqwest 0.13.5, Rusqlite 0.40.2, Keyring 4.2.0, Rand 0.10.3 and pdfium-render 0.9.4. The unused opener package was removed. Ocrs 0.13.1 and RTen 0.26.0 remain current; development inference kernels are optimized so OCR is tested in the normal Rust suite.

Pdfium was updated from 156.0.8066.0 to **156.0.8076.0**. Its official archive digest was checked before extraction. OCR model hashes match the pinned Hugging Face LFS metadata; the model card explicitly declares CC-BY-SA-4.0. Roboto is pinned to an upstream Roboto 2 commit with the Apache-2.0 license. All relevant notices are included in the desktop bundle. See `src-tauri/resources/manifest.json` and `THIRD-PARTY-NOTICES.md`.

## Advisory results and external limitations

JavaScript audit: **zero known advisories**. The Rust all-platform lockfile audit reports **zero entries in the vulnerability category and two informational warnings**, reduced from nine. The unsupported Linux GTK dependency chain still includes:

| Package | Advisory | Status |
| --- | --- | --- |
| `glib` 0.18.5 | [RUSTSEC-2024-0429](https://github.com/rustsec/advisory-db/blob/main/crates/glib/RUSTSEC-2024-0429.md) | Unsound iterator implementation, fixed upstream in 0.20+. Tauri's Linux GTK API still requires the older dependency chain. |
| `proc-macro-error` 1.0.4 | [RUSTSEC-2024-0370](https://github.com/rustsec/advisory-db/blob/main/crates/proc-macro-error/RUSTSEC-2024-0370.md) | Unmaintained GTK macro dependency. |

`cargo tree --target x86_64-pc-windows-msvc -i glib -i proc-macro-error --locked` has no matching dependency path in the supported Windows build. These advisories are not suppressed. Builds for other targets are explicitly rejected until native resources and their dependency security review are supplied. Upgrading those transitive GTK major versions blindly would break the upstream API.

Remaining product boundaries are explicit: Pdfium parses native documents inside the application process; limits and serialization do not provide a separate process sandbox. Cancellation stops between pages, not inside a native parser/OCR operation. Snapshot encryption does not protect OS paging, crash dumps, historical plaintext blocks, original documents or intentional named exports. Portable recovery requires a previously exported encrypted backup and its password. Trusted installer signing and model quality on real handwriting require external deployment resources and validation. Model-generated advice still requires teacher review; a passing synthetic case is not a quality guarantee.

## Validation

- `pnpm test`: **5 passed**, covering Unicode byte ranges, score edits/revisions, dialog failures and restoring the actual assignment rubric.
- `pnpm build`: **passed**, with TypeScript 7 and the updated Tauri API.
- `pnpm verify:resources`: **passed** for the native binary, embedded font and both OCR models.
- `pnpm audit --json`: **zero known advisories**.
- `cargo fmt --all -- --check` and `git diff --check`: **passed**.
- `cargo test --lib --locked`: **38 passed, 0 failed, 5 ignored**. The scanned-PDF OCR test runs and passes in this suite. The ignored tests depend on Ollama or shared OS credentials.
- `cargo test --lib -- --ignored --skip is_idempotent --test-threads=1`: **4 passed, 0 failed**, against the available local Ollama 0.34.4 and `qwen3:8b`. This covers connectivity, incorrect teacher assessment, instruction injection and verbatim physics evidence. The final inference configuration completed the four checks in about 81 seconds after earlier requests exhausted the deadline.
- Original Spanish schema migration: **passed**, including preserved foreign-key references and translated confirmed-grade state.
- Rust advisory audit: **zero vulnerability-category entries; two visible GTK informational warnings**, detailed above.
- PDF inspection: generated and rendered a ten-page Unicode/long-comment report and a confirmed teacher grading report; checked first/last-page readability, final content and all-page text bounds. No text crossed the safe page margins.
- Windows desktop integration: opened the bundled frontend with CSP enabled; created synthetic rubric/assignment records, imported a real fixture through native IPC with progress, reviewed redaction, edited/saved scores and comments, reloaded the screen, confirmed the teacher grade and exported grade/audit CSV and feedback PDF. Protected export paths were rejected. Native file dialogs opened correctly; actual selection/save interactions were exercised through IPC because the automation provider cannot target keyboard input in the owned dialog window. QA uses an isolated debug data directory and synthetic names. Restart preserved the confirmed grade, all scores and the accented comment; repeated native-PDF imports and a scanned-PDF OCR import succeeded. Cancellation during OCR returned an error without inserting a submission, and page progress reached the saved state on success.
- `pnpm tauri build --debug --bundles nsis`: **passed**, producing a Windows x64 NSIS test installer with resources and notices. This verifies debug packaging and the bundled frontend; a signed production release installer has not been validated.

The OS credential-store idempotence unit test remains intentionally ignored to avoid manipulating shared credentials. Application startup exercised the actual Windows credential store successfully. The project is now hosted in the public [gafapa/corregir repository](https://github.com/gafapa/corregir), with [Windows verification on GitHub Actions](https://github.com/gafapa/corregir/actions/workflows/ci.yml). Hosted checks cover the default frontend/backend suites, resource integrity, frontend compilation, formatting and dependency audits. Ollama integration and interactive desktop checks above were performed locally and are excluded from hosted CI.

The [first hosted verification run](https://github.com/gafapa/corregir/actions/runs/36690599926), for commit `de7b0aa9d608aad39c606c24751e533d48b55a15`, completed successfully on 2026-09-30. It passed 5 frontend tests and 38 Rust tests (5 environment-dependent tests ignored), including scanned-PDF OCR, as well as frontend compilation, resource hashes, Rust formatting and both dependency audits. JavaScript reported no known vulnerabilities; Rust reported the same two GTK informational warnings listed above. GitHub saved the Rust dependencies and installed audit tool in its cache. Subsequent documentation or CI changes are recorded in the workflow history linked above.

Repository security alerts and automatic security updates are enabled. GitHub Dependabot independently confirmed that the `glib` security update cannot be resolved within the current GTK dependency requirements (`security_update_not_possible`, installed/resolvable 0.18.5 versus fixed 0.20.0). The alert remains open; the supported Windows build does not include this package. Dependency update pull requests require a passing Windows verification run before integration.

The initial dependency update proposals exposed breaking APIs in digest hexadecimal formatting and nonce generation. The implementation now migrates production `sha2` and `aes-gcm` to 0.11, with explicit byte formatting and random nonce generation. A test-only alias of `aes-gcm` 0.10 verifies that existing snapshot bytes remain readable and newly persisted snapshots retain the original wire format. Argon2, ZIP and XML libraries support the new portable-backup and DOCX features. Hosted verification for these changes is recorded separately below.

## Continued implementation (2026-10-02)

- Custom rubric creation and immutable edited versions, with regression checks for existing assignment scores and stale version requests.
- Persisted local Ollama server/model settings, used by both grading requests and diagnostic connection tests.
- Text-only DOCX imports with bounded decompression/XML parsing and explicit rejection of content requiring full Word rendering.
- Password-protected portable backups, empty-workspace restore and explicit startup recovery that archives the original encrypted snapshot.
- A manually dispatched Windows release installer workflow. Its output is unsigned until a signing certificate and process are provisioned.
- Browser previews with synthetic IPC fixtures cover the new editor, settings, narrow dark-mode backups and credential-recovery screen. No horizontal overflow, undersized buttons or JavaScript exceptions were observed; keyboard skip navigation and text contrast checks passed. These previews do not substitute for native credential-recovery or installation testing.

Real handwriting evaluation, macOS/Linux resource bundles, signed public distribution and institutional deployment approval remain outside the implemented Phase A scope.

For commit `3b5c978afc5d97383c55165639e86b4ffc45ab42`, [hosted verification](https://github.com/gafapa/corregir/actions/runs/36934171545) completed successfully: **12 frontend tests and 48 Rust tests passed**, with five environment-dependent Rust tests ignored. Frontend compilation, resource verification, formatting and both dependency audits passed; the same two upstream GTK warnings remain visible. Checks ran on GitHub rather than the local development machine.

The [Windows installer run](https://github.com/gafapa/corregir/actions/runs/36934177062) also completed successfully for that commit. It repeated the frontend/backend suites, built the release-profile NSIS installer, installed it silently on the hosted runner, verified installed resource hashes and started the packaged application twice against the same encrypted workspace without creating a plaintext SQLite file. This checks packaging, initial startup and reopening with the OS credential store; it does not verify interactive grading or trusted code signing. The unsigned installer is available in that run's artifact for 14 days.

## Primary references

- [Ollama thinking API](https://ollama.com/blog/thinking), including the top-level `think` option.
- [Ollama structured outputs](https://docs.ollama.com/capabilities/structured-outputs).
- [Ollama API](https://github.com/ollama/ollama/blob/main/docs/api.md), including bounded generation options.
- [Pdfium binary release](https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium%2F8076).
- [Ocrs model card](https://huggingface.co/robertknight/ocrs).
- [RustSec cargo-audit](https://github.com/rustsec/rustsec/tree/main/cargo-audit).
