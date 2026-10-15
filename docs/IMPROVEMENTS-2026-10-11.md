# Classroom workflow, languages and data lifecycle — 2026-10-11

This follow-up implements the product review of the same date. Earlier reviews concentrated on security and robustness; this one addressed what blocked a real class: a verified evidence bug, the per-submission workflow, the English-only interface and the missing data-lifecycle tools required by the DPIA. Validation used synthetic fixtures, temporary work directories and the local `qwen3:8b` model. No student data, shared credential-store tests, hosted workflow, packaging or commit were involved.

## Verified defects fixed

| Defect | Evidence | Change |
| --- | --- | --- |
| Evidence requests failed on real extracted text | With the application's exact prompt and schema, `qwen3:8b` joined OCR line breaks with spaces; 4 of 5 quotes failed the exact-substring check and the whole response was rejected. Deterministic sampling made retries identical. The existing live test used single-line text. | Quotes are matched after folding whitespace, hyphenated line breaks, typographic quotes/dashes and letter case; the stored quote is the exact excerpt of the redacted text. Unmatched quotes are discarded and counted, repeated criteria merged and missing criteria returned empty. Unit regression with the observed quotes and a live regression with multi-line Galician OCR text. |
| Reports called unreviewable feedback "teacher-reviewed" | The PDF printed the AI draft under "Teacher-reviewed AI draft", but no command could edit it. | Feedback is an editable field saved as `feedback_final` with an AI-assistance flag and an audit event. Reports contain only that text and state when an AI draft was used. Confirmation saves pending edits first. |
| Feedback PDF export was not audited | CSV export recorded an event; the PDF, which contains a name and grade, did not. | Single and batch PDF exports record `export_pdf_feedback`. |
| Spreadsheet export unsuitable for Spanish-locale spreadsheets | No byte-order mark, comma separator and all criteria in one cell. Float sums could print `7.300000000000001`. | One column per criterion, total, maximum and grade out of 10, sorted by student, numbers rounded to two decimals, UTF-8 BOM; Galician/Spanish use semicolons and decimal commas. |
| Roster detection could panic | `detect_by_roster` sliced the original text with offsets found in a lowercased copy; characters such as the Kelvin or ohm sign change UTF-8 length when lowercased. With `panic = "abort"` this would end the process. | Case-insensitive matching on the original text; regression test with U+212A. |
| Restored backups skipped migrations | `restore` and recovery installed the decrypted connection directly. | Schema migrations run on every decrypted backup before verification or restore. |

## Implemented behavior

| Area | Result |
| --- | --- |
| Classes | Rosters per class, pasted one name per line; duplicates and spacing are normalized. Assignments can be linked to a class. Rosters are encrypted with the workspace and audited by counts only. |
| Submissions screen | List of the assignment's submissions beside the selected one, with status, search by name/file/number/text and "next pending". Multiple documents per import, processed sequentially with per-file and per-page progress and a stop button; failures are listed without stopping the batch. Source file names are kept in the encrypted workspace to tell submissions apart. |
| Redaction | Detection runs automatically. The student is chosen from the class roster; a suggestion comes from the file name and detected roster names. The backend refuses a roster student outside the assignment's class. |
| Grading | Rubric performance levels (optional, up to 10 per criterion) fill the score when clicked. Live total and grade out of 10. Evidence for all ready submissions in one action. Feedback prompts include subject, grade level, levels and the teacher's comments, address the student in the second person and use a configurable language (default: the student's language). Teacher comments have the student's full name and name parts replaced with the alias first. Confirmed grades can be reopened with an audited reason. |
| Deletion | Submissions, assignments, unused rubric versions and classes, after a confirmation dialog; audited without names. |
| Data lifecycle | Retention period with startup/daily reminders; per-assignment identity purge; per-student JSON access export and erasure. |
| Languages | Galician, Spanish and English interface, reports and spreadsheets. Initial language from the system locale, then the teacher's choice. Known backend messages are translated. TypeScript enforces identical message keys in the three dictionaries. |
| Prompt hardening | Text cannot open or close the prompt's delimiting tags. Token budgeting uses a character-class estimate instead of counting bytes, measured against `qwen3:8b` prompt counts, so ordinary multi-page answers are no longer rejected. |
| Schema | Version 4 (`PRAGMA user_version`); per-submission metadata moved from configuration keys to columns so deletions cascade. |
| Interface text | Internal milestone names removed from the interface and window title. |

## Validation

- Frontend: 42 tests across 13 files passed; TypeScript and the Vite production build passed.
- Rust: 80 library tests and 5 real-executable integration tests passed with no warnings; formatting check passed. Live checks against local Ollama `qwen3:8b` (connection, injection resistance, verbatim physics evidence, inconsistency detection and the new multi-line Galician OCR regression) passed: 5 of 5.
- Real application end to end: the debug build with a fresh password-protected portable work directory, driven through the Galician interface over WebView2 remote debugging and the real local model. Class creation (duplicate removed), rubric with levels, assignment linked to the class, three synthetic imports through native IPC (native, text and scanned OCR), automatic detection and roster suggestion, redaction preview without name/ID/email, evidence (2 verbatim quotes, none discarded), level-button scoring, feedback draft, edited feedback saved on confirmation, Galician CSV/PDF/batch PDF exports, reopening and reconfirmation, access export, identity purge (teacher comment no longer names the student), erasure and audit payloads without names. The work directory contained only the encrypted database, key envelope and lock after closing, and no tested name occurred in the encrypted snapshot. File dialogs and confirmation dialogs are native and were bypassed through the same IPC commands.
- Visual check with injected synthetic IPC: 12 screenshots covering every section, Galician/Spanish/English, 1280-pixel and 390-pixel widths and dark mode; no horizontal page overflow or runtime errors.
- Qualification scripts were updated to the new interface and pin English; they were syntax-checked, not executed. No dependency changed, so audits were not repeated.

## Release preparation

The limits recorded after the first validation were addressed the same day and the version was raised to 0.4.0.

| Earlier limit | Change |
| --- | --- |
| Teacher comments hid only the confirmed student's name | The full names of the assignment's other students and of its class roster are replaced by `[…]` in any case, and their single name parts when capitalized; many surnames and given names are also ordinary words, such as Vila or Mar. Identity purge rewrites all texts before removing any name link. Erasure replaces the erased student's name in other submissions' comments, saved feedback and AI drafts and reports how many texts changed. The interface asks teachers to avoid nicknames, which remain undetected. |
| Confirmation dialogs could not be automated | The isolated qualification build substitutes `ask` as well as the file pickers. The substitute records each message and returns a configured answer, which defaults to cancel. |
| New features were not covered by installed-application qualification | `qualify-desktop.cjs` now covers a class roster with a duplicate, a rubric with suggested levels, an assignment linked to the class, the roster suggestion after a text correction, level scoring, a teacher comment naming the student and a classmate, edited feedback, reopening and reconfirmation, CSV/single/batch PDF exports, a cancelled and a confirmed deletion, a per-student access export, the Galician interface and the expected audit events. The test model server checks that neither name reaches it and that the classmate appears as `[…]`. |
| Roster detection flagged ordinary words (found by the first qualification run) | Detection matched every roster name part as a case-insensitive substring, a behaviour inherited from 0.3.0 that mattered little while teachers typed one name. With the whole class roster supplied automatically, a part such as "Student", "Ana" or "Mar" was preselected inside "análise", "o mar" or "marco", and particles of three letters such as "del" matched everywhere. Full names now match as whole words in any case and across line breaks; single parts match only as capitalized whole words, and particles are ignored. |
| Executable named `tauri-app.exe` | The Cargo package is `corregir`, so the executable is `corregir.exe`. The Tauri NSIS template stores the main binary name. On upgrade it deletes the previous binary and retargets shortcuts that pointed to it. The portable ZIP keeps `Corregir.exe`. |
| Downgrades | Migration refuses a `user_version` newer than the application's, for workspaces and backups, without changing the data. This applies from 0.4.0; 0.3.0 has no such check, so the release notes ask teachers not to reopen upgraded data with it. |
| Version | 0.4.0 in `package.json`, `Cargo.toml`, `Cargo.lock` and `tauri.conf.json`; release notes in [RELEASE-0.4.0.md](RELEASE-0.4.0.md). |

### Release validation

- Frontend: 42 tests across 13 files passed. TypeScript and the production build passed.
- Rust: 83 library tests and 5 real-executable integration tests passed, and the formatting check passed. The 5 live checks against local Ollama `qwen3:8b` passed. Clippy with the qualification feature reports only warnings that predate this work.
- Installed-application qualification (`pnpm qualify:windows`) passed all five phases on this workstation: create, reopen, upgrade from 0.1.1 to 0.1.2, restore into a fresh workspace, and reopen the restored data. It then uninstalled the qualification product and archived its isolated workspaces. Evidence: `output/desktop-qualification/20261011-231944-9b1c0784`.
  - The first attempt stopped at the deletion step. The dialog plugin's `ask` calls `plugin:dialog|message`, not `ask`, so the substitute was corrected.
  - The same run also exposed the roster-detection false positive described above.
  - That attempt's workspace was archived inside its run directory and its installation was uninstalled.
  - Before the full rerun, the create and reopen phases were checked against a debug qualification executable.
- Normal 0.4.0 NSIS installer built: `Corregir_0.4.0_x64-setup.exe`, SHA-256 `902d1ec686bba98c3818666e27fd3c81fd956fca351ae66094bcedfbc910efc8`. It was not installed on this workstation, so the existing normal workspace and credential were left untouched; the hosted packaging workflow installs it on a disposable runner.
- Normal portable ZIP `Corregir_0.4.0_windows_x64_portable.zip`, SHA-256 `7120dbefa582a0418a73209781546cdfbb5dafc8977d3eaf1ac0fb93d914f534`.
  - `verify-portable.ps1`: resource hashes, metadata and native and mixed-OCR extraction after relocation passed.
  - `verify-work-directory.cjs` passed six phases: create, wrong password, reopen, relocate, restore into a new directory and reopen the restored data. The normal personal database was unchanged.
- No dependency changed, so the dependency audits were not repeated.

### Code review before publishing

A review of the complete change on 2026-10-16 found the following, fixed before the commit. The installer and ZIP hashes above belong to the build before these fixes; the artifacts to distribute are the ones produced by the hosted packaging workflow from the pushed commit.

| Finding | Change |
| --- | --- |
| Erasure replaced every capitalized part of the erased name in all remaining submissions, including another student's own feedback when both shared a given name | The scrubber of each remaining submission keeps the parts of the erased name that also belong to that submission's own student; full names are still replaced. |
| AI drafts could address the student by the random alias, and the draft becomes the student-facing feedback | After the model call, the alias is replaced locally by the student's name in the draft and the observations. |
| The reason for reopening a grade was stored verbatim in the audit trail, which outlives purges and erasure | Known names in the reason are replaced as in comments sent to the model: the alias for the student and `[…]` for others. |
| A failure partway through a batch PDF export left reports on disk with no audit record | The reports already written are audited, with `complete: false`, before the error is reported. |
| A capitalization fix in a class roster was ignored | The existing student keeps its identifier and takes the corrected spelling. |
| Grade spreadsheets sorted accented names after "z" | Names sort ignoring accents, with "ñ" after "n". |
| The comment redactor was rebuilt for every criterion | It is built once per feedback request. |

Rust: 84 library tests and 5 integration tests pass; frontend 42 tests and TypeScript pass. Two findings were kept as documented limits below. A third, marking saved feedback as AI-assisted whenever a current AI draft exists, is intended: it may over-disclose AI assistance but never hides it.

## Remaining limits

- Hosted CI and the hosted packaging workflow have not run; they need the changes to be committed and pushed.
- Native confirmation and file dialogs of the normal build remain outside automation; the qualification build substitutes them.
- Nicknames and other free-text identifiers in teacher comments are not detected.
- Access and erasure requests identify a student by name. Two different students with the same name are treated as one; the Privacy section shows how many submissions and roster entries each name has, which the teacher should check first.
- An identity purge removes the names of the assignment's confirmed students. Submissions still pending can later redact those names only if the class roster lists them.
- Retention periods and the handling of access/erasure requests must be defined with the DPO. Earlier encrypted backups keep deleted records until destroyed.
- Executables are unsigned.
