# Corregir 0.4.0

This release makes the prototype workable for a whole class and adds the data-lifecycle tools required by the impact assessment. It remains the Phase A synthetic-data prototype and should be published as a prerelease.

## Downloads

- Windows x64 no-install ZIP: extract the complete archive and run `Corregir.exe`, as in 0.3.0.
- Windows x64 NSIS installer: the installed executable is now `corregir.exe`. Upgrading from an earlier installer removes `tauri-app.exe` and retargets its Start menu and desktop shortcuts.
- SHA-256 files accompany the downloads. Both executables are unsigned because no signing certificate is available.

WebView2 must be available on the computer. OCR resources are included. Ollama and its language model are separate local prerequisites.

## Changes

- **Fixed:** evidence requests failed on real extracted text because the model reflows line breaks. Quotes are now matched after folding whitespace, hyphenation, typographic quotes and case, and stored as exact excerpts. Unmatched quotes are discarded and counted.
- **Fixed:** feedback reports called an uneditable AI draft "teacher-reviewed". Feedback is now an editable field; reports contain only the saved text and state when an AI draft was used.
- **Fixed:** PDF exports were not audited, spreadsheets were unsuitable for Spanish-locale software, roster detection could crash on some Unicode characters and flagged name parts inside ordinary words, and restored backups skipped schema migrations.
- **Classes:** rosters pasted once per group, student selection and suggestion during redaction, list-and-detail submissions screen, multi-document import and evidence for every ready submission.
- **Grading:** rubric performance levels, live total and grade out of 10, reopening a confirmed grade with an audited reason, batch PDF export.
- **Languages:** Galician, Spanish and English interface, reports and spreadsheets.
- **Privacy:** retention reminders, identity purge per assignment, per-student access export and erasure. Teacher comments reach the model with the student's name replaced by the alias and classmates' names hidden. Erasure also removes the erased student's name from other students' comments and feedback.
- **Deletion:** submissions, assignments, unused rubric versions and classes, after confirmation.

## Upgrading

1. In the current version, export a password-protected backup and verify it.
2. Close Corregir and install or extract 0.4.0.
   - With the installer, run the new installer.
   - With the ZIP, extract it and select the existing work directory at startup, or move the `work` folder beside the new `Corregir.exe`.
3. The first opening upgrades the workspace to schema version 4.
   - Document fingerprints move from configuration entries to submission columns.
   - AI drafts of grades that were already confirmed become their saved feedback, marked as AI-assisted.
   - Backups from earlier versions are upgraded in the same way when verified or restored.

After the upgrade, do not open the work directory with 0.3.0 or earlier: those versions do not recognize the new data. From 0.4.0 on, the application refuses workspaces and backups saved by a newer version, leaving them unchanged. Keep the backup made before upgrading until 0.4.0 has been checked with your data.

## Validation

See [IMPROVEMENTS-2026-10-11.md](IMPROVEMENTS-2026-10-11.md#release-preparation) for the evidence recorded for this version.

## Limits

- Nicknames and other identifiers typed in teacher comments are not detected automatically.
- Access and erasure requests find a student by name, so two students with the same name are treated as one. Check the number of submissions and roster entries shown before erasing.
- After an identity purge, the purged students' names are known only from the class roster when the assignment's pending submissions are graded.
- Retention periods and the handling of access and erasure requests must be defined with the DPO. Encrypted backups created before a deletion keep the deleted records until they are destroyed.
- Second-machine provisioning, removable or network volumes, other platforms, real handwriting and production deployment remain outside the recorded evidence.
