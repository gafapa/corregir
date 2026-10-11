# Improvements — 2026-10-10

The later [desktop qualification and OCR review](DESKTOP-QUALIFICATION-2026-10-10.md) adds AppContainer restrictions, source-page comparison, editable extraction and installed-application validation. Limits and counts below describe the preceding implementation.

## Implemented behavior

This local, uncommitted follow-up addresses the six requested improvements using synthetic fixtures only. No student data, shared credential-store tests, deployment or hosted workflow execution was involved.

| Area | Result |
| --- | --- |
| Durable persistence | Every production workspace mutation uses a staged SQLite connection. Only a successfully persisted encrypted snapshot is adopted. Closure, serialization/write, synchronization or replacement failure leaves the active connection and prior disk snapshot unchanged. Staging temporarily requires an additional database copy in memory. |
| Mixed PDFs | Pages containing images or form objects use full-page OCR even when a native text header is present. Teachers can force OCR on every PDF page and compare extracted answers against the original. A mixed-page synthetic fixture exercises the real executable. |
| Redaction | Case-insensitive exact full-name matches are included automatically. The interface displays the resulting submission text and requires an explicit review checkbox. Editing the name or redaction inputs invalidates the preview. Confirmation recomputes the exact preview in the backend and rejects mismatches. Variants, OCR errors and indirect identifiers still require teacher review. |
| Document worker | Parsing runs in a separate instance of the application executable, before Tauri, workspace or credential-store initialization. Private bounded pipes carry progress and results. Cancellation kills the child; malformed documents, crashes and timeouts do not insert partial submissions. Windows Job Object limits: one process, 1 GiB committed memory and termination on job closure. The child inherits only SystemRoot, with its working directory set to the executable directory. |
| Inference | Evidence, feedback and diagnostics share one execution slot and at most three waiting slots. The interface reports queued, running and saving states. Cancellation releases waiting capacity or drops an active HTTP future; an early cancellation cannot race past registration. Saving is allowed to complete durably. |
| Backups | Export authenticates and validates the portable backup before writing. Encrypted health metadata records its path, fingerprint, creation date and recovery-check date. Startup and daily reminders flag missing, changed or week-old copies. A password-entered recovery check validates a selected backup without restoring or replacing current records. Password fields clear after successful operations and passwords are never stored. |

The Job Object follows the [Windows Job Objects lifecycle and resource controls](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects). The worker still has the current account's filesystem and network permissions: this is process containment, not a restricted-token or filesystem/network sandbox. Operating-system paging and crash dumps are outside the encrypted-snapshot guarantee. Cancellation closes the Ollama client request but cannot guarantee that the server stops its internal generation.

## Validation

- Frontend: 17 tests passed across eight files; TypeScript and Vite production build passed.
- Backend: 59 library tests and three real-executable integration tests passed; five tests requiring Ollama or shared OS credentials remain explicitly ignored.
- Persistence regressions cover injected write/sync/replace failures, closure failure, successful retry, reopening the encrypted workspace, and an actual Windows file-sharing lock that prevents atomic replacement.
- Document regressions cover native extraction, a mixed page whose scanned answer must survive OCR, forced OCR, cancellation during extraction and a malformed PDF without host termination.
- Queue regressions cover running and waiting cancellation, capacity release, dropped HTTP work and cancellation before request registration. Redaction regressions include Unicode text and preview invalidation. Backup regressions cover missing/changed/old copies and validation without restoration.
- Resource verification passed for Pdfium, Roboto and both OCR models. JavaScript audit reports zero known vulnerabilities. Cargo audit scanned 600 locked dependencies with zero vulnerability-category findings and two existing informational GTK warnings: RUSTSEC-2024-0370 and RUSTSEC-2024-0429. Neither enters the supported Windows target; advisories remain visible.
- Synthetic browser preview using the actual React interface and mocked native IPC exercised reviewed redaction, queued cancellation and backup verification without restoration. Desktop and 600-pixel layouts were inspected; no horizontal overflow or browser page errors were detected. This is UI verification, not a native WebView integration run.
- CI now runs the executable integration tests. The installer workflow adds extraction through the installed executable. Workflow syntax is checked locally; these new packaging checks have not run on hosted CI.

## Remaining qualification

A new release installer has not been built or installed in this follow-up. Packaging, code signing, native WebView end-to-end behavior, real handwriting quality and institutional deployment remain unqualified. The installer is still unsigned and Windows-only. Incompatible OCR/font-library and package-manager major updates remain deferred as described in the preceding review.
