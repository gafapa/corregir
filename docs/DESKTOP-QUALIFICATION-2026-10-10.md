# Desktop qualification and OCR review — 2026-10-10

## Changes

- Windows document parsing now runs in an AppContainer with no network capabilities. The parent passes only a whitelisted read-only source handle and private pipes; no source copies are written and source-file ACLs are untouched. A Job Object applies memory/process limits before the suspended child resumes. Unique-profile read grants on public executable/OCR resources are revoked afterwards, without discarding concurrent ACL changes. Processing fails closed if the sandbox cannot launch. Standard Windows profile-path variables are allowlisted; PATH and credentials are excluded.
- The worker hashes the exact source bytes used for extraction/rendering. Imports record the SHA-256 inside the encrypted workspace. Selecting an original for comparison must match that fingerprint, including the worker's actual input; a changed or different source is rejected.
- “Compare original and extracted text” shows a bounded PDF/image page next to editable text. PDF page navigation is local and rendering remains in the restricted process. Zoom from 100% to 300% uses an internally scrollable, keyboard-focusable preview so enlarged pages do not widen the application. Preview output is bounded and its PNG dimensions are checked before displaying it. DOCX comparison uses the user's document editor. Older submissions without fingerprints can still be graded and corrected; in-app original-page comparison requires a new import.
- Text correction is permitted only before redaction. Saving checks the exact previously loaded text, records an audit event without text payloads, retains the initial extraction inside the encrypted workspace and persists atomically. Corrections invalidate redaction candidates/previews. Unsaved corrections block redaction, filtering and tab changes until saved or discarded.
- Local search matches submission number, known name or extracted text. Status filters and “Next pending submission” help teachers navigate unfinished assessments with keyboard focus.
- Reusable Windows qualification scripts build/install two isolated versions, verify installed resources, exercise the native UI/IPC and HTTP boundary, reopen saved work, upgrade without replacing records, validate a portable backup, restore into a fresh workspace and reopen restored records.
- The qualification build has its own application identifier and credential entry. Its file-picker substitute is compiled only with the qualification feature and accepts only the explicitly configured synthetic source/output paths. It does not weaken the production IPC function, which Tauri makes non-writable. The runner refuses to start an executable whose product metadata is not the qualification product and checks its native identifier/version before mutating records.
- Signing tooling checks code-signing usage, private-key availability, certificate validity, timestamping and final signature validity. It requires a separately provided certificate; no signing credentials are bundled.

AppContainer permits necessary Windows resources according to OS ACLs, so this does not imply that literally every other system file is inaccessible. The observed deny tests cover an unrelated private file, reopening the selected source by path and loopback networking. This is not protection against an administrator or compromised OS. See [Microsoft AppContainer isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation) and [launch requirements](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer).

## Verification

| Check | Result |
| --- | --- |
| Frontend | 20 tests passed across nine files, including correction/version arguments, post-redaction read-only text and filtering. Normal `pnpm test` and TypeScript/Vite build passed. |
| Default Rust suite | 60 library tests and five real-executable integration tests passed; five environment-dependent tests remain ignored in the default command. |
| Live inference | All four Ollama checks passed against local `qwen3:8b`: connection, verbatim evidence, prompt-injection resistance and inconsistency detection. The shared OS credential test was explicitly excluded. |
| Sandbox | Native/mixed PDF extraction, cancellation, preview and malformed-input tests passed inside the AppContainer. A synthetic sentinel and a listening loopback server verified denied private-file access, source-path reopening and network connection. |
| Dependencies/resources | JavaScript audit: zero known vulnerabilities. Cargo audit: 600 locked crates, zero vulnerability-category findings and the two existing GTK informational warnings. Pdfium, font and both OCR hashes verified. |
| Installed workflow | All five phases passed: clean 0.1.1 installation and assessment creation; reopening; upgrade to 0.1.2; backup restoration into an empty workspace; reopening restored records. The confirmed assessment retained score 8, revision 1 and its feedback throughout. CSV/PDF exports, encrypted-at-rest identity checks and the local inference boundary passed. Installed resource hashes matched the manifest. |
| UI review | Desktop and 600-pixel viewport captures inspected. Zoom at 200% scrolls inside the preview; the application has no horizontal page overflow at the narrow width. |
| Normal installer | `pnpm tauri build --bundles nsis` passed without the qualification feature. The application metadata is `Corregir`, version `0.2.0`. The generated NSIS installer is unsigned. |

The installed-flow runner uses WebView2's per-process [debugging configuration](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/debug-visual-studio-code). Only file picker selection is substituted; application commands, encrypted persistence, parsing, redaction, grading and exports run in the real installed executable. A deterministic localhost Ollama-compatible server separately asserts that the entered synthetic identity is excluded from prompts. Live-model tests are listed separately above. Qualification uses synthetic data and separate profiles; the normal workspace and shared credential are not used.

The normal installer is `src-tauri/target/release/bundle/nsis/Corregir_0.2.0_x64-setup.exe` (18,923,348 bytes). SHA-256: `9ca104d7241a69949abdf68047bf46084a226ad4f437ad38984e102c228ebae7`. Authenticode status is `NotSigned`. It was built and its application metadata checked; installation/upgrade/restore evidence above comes from the separate qualification product, including its controlled picker substitute. The normal installer was not installed over the existing normal workspace.

## Reproduce

```sh
pnpm qualify:windows
```

The script requires an absent `com.corregir.qualification` data profile, builds the 0.1.1 qualification installer, creates a synthetic confirmed assessment, reopens it, installs qualification version 0.1.2, verifies the same assessment, archives the test workspace and restores its portable backup into a fresh test profile. It then reopens the restored assessment, archives that encrypted workspace and uninstalls only the qualification application. Each future run uses a unique evidence directory under `output/desktop-qualification`. These are qualification versions built from the current source tree, not a historical-release compatibility matrix.

The manually dispatched `qualify.yml` workflow runs the same checks and retains only synthetic evidence for seven days. No hosted workflow, commit, tag or deployment was performed in this local review. This local run's five `result-*.json` files, screenshots, backup, exports and both qualification installers are under `output/desktop-qualification`. Cleanup was also executed locally: both successful encrypted workspace snapshots are archived, and the qualification installation was removed. The separate qualification credential entry is retained for subsequent runs; the normal credential/profile were not used.

## Remaining limits

Signing requires a valid certificate with its private key; none was found in the current user's code-signing certificate store. Hardware-backed certificates elsewhere have not been inspected. Real handwriting quality, institutional authorization, cross-platform behavior and deployment remain outside this qualification. The prototype retains its synthetic-data scope.
