# Changelog

Detailed evidence for each change is in the dated documents under `docs/`.

## 0.4.0 — 2026-10-11

Classroom workflow, languages and data lifecycle. See [docs/RELEASE-0.4.0.md](docs/RELEASE-0.4.0.md) and [docs/IMPROVEMENTS-2026-10-11.md](docs/IMPROVEMENTS-2026-10-11.md).

- Fixed: evidence requests no longer fail when the model reflows extracted text; unmatched quotes are discarded and counted.
- Fixed: roster detection could panic on characters whose lowercase form changes length.
- Fixed: restored backups now run schema migrations.
- Fixed: roster detection matched name parts inside other words and in lowercase ("Ana" in "análise", "mar"); parts now match only as capitalized whole words, full names in any case and across line breaks.
- Added: Galician and Spanish interface, reports and spreadsheets.
- Added: class rosters, student selection and suggestion during redaction.
- Added: list-and-detail submissions screen, multi-document import and evidence for all ready submissions.
- Added: rubric performance levels, live total and grade out of 10.
- Added: editable, audited feedback; reports state AI assistance. Feedback prompts use teacher comments with names redacted.
- Added: reopening confirmed grades with a reason; deletion of submissions, assignments, unused rubrics and classes.
- Added: retention reminders, identity purge, per-student access export and erasure.
- Changed: grade spreadsheets have one column per criterion, a grade out of 10, BOM and locale separators; PDF exports are audited and can be exported in batch.
- Changed: schema version 4. Data saved by a newer version is refused unchanged.
- Changed: teacher comments sent to the model also hide the names of the assignment's other students and the class roster; erasure removes the erased student's name from other students' comments and feedback.
- Changed: the executable is `corregir.exe`; installer upgrades remove `tauri-app.exe` and retarget its shortcuts.
- Fixed before release: erasure kept other students' own names when they share a part with the erased name; AI drafts no longer show the alias; reopening reasons and partially failed batch exports are audited without names; roster capitalization fixes are kept; spreadsheets sort accented names correctly.
- Changed: installed-application qualification covers rosters, levels, reopening, batch export, deletion confirmations, access export and the Galician interface.

## 0.3.0 — 2026-10-11

Password-protected portable work directories. See [docs/RELEASE-0.3.0.md](docs/RELEASE-0.3.0.md) and [docs/WORK-DIRECTORY-2026-10-11.md](docs/WORK-DIRECTORY-2026-10-11.md).

## 0.2.0 — 2026-10-10

AppContainer document worker, original-page comparison, editable extraction, installed-application qualification. See [docs/DESKTOP-QUALIFICATION-2026-10-10.md](docs/DESKTOP-QUALIFICATION-2026-10-10.md) and [docs/IMPROVEMENTS-2026-10-10.md](docs/IMPROVEMENTS-2026-10-10.md).

## Earlier

Phase A implementation, security review and remediation. See [docs/PROJECT-REVIEW.md](docs/PROJECT-REVIEW.md) and [docs/REVIEW-2026-10-09.md](docs/REVIEW-2026-10-09.md).
