# Password-protected portable work directories — 2026-10-11

## Delivered behavior

The normal portable ZIP now starts with a work-directory selector and password gate. Its default is `work` beside `Corregir.exe`; another directory can be chosen at startup. Successful opening remembers the selected location atomically. Locations inside the application folder are saved relatively, so moving the complete folder retains them. Changing the active directory requires closing the application and choosing the next directory on the following launch.

The directory contains the encrypted SQLite snapshot and an 84-byte `corregir.workspace-key` envelope. A random 32-byte database key is wrapped with an Argon2id-derived password key (64 MiB, three iterations, one lane) using AES-256-GCM. The versioned header, salt and nonce are authenticated. Passwords and unprotected keys are not written. Portable opening uses no Windows credential-store calls. The normal installed workspace retains its existing Windows-bound behavior.

Directory locking occurs before key creation or reading the database. Incorrect passwords, invalid key sizes and tampered envelopes stop before database opening. A directory containing an encrypted Windows-bound snapshot without the portable key, or any legacy plaintext database files, is refused without replacement. Existing work is transferred through verified password-protected backup export/restore into a new portable directory; there is no automatic conversion or merge.

The work-directory password and a backup password are separate protections. Losing the work-directory password or its protected key requires a verified backup and that backup's password, restored into a new directory. Original documents remain at their selected source paths and are not bundled into the work directory. WebView2 runtime/browser data still uses its own profile; this is not a claim that every application cache or system dependency is folder-local.

## Artifact

- `output/portable/Corregir_0.3.0_windows_x64_portable.zip`
- Size: 21,853,375 bytes.
- SHA-256: `633cab05901905d43232652e77c649f8de01250db3e1660e6b64dd7584aef746`.
- Normal Windows x64 application and OCR resources, with no personal work or credentials included. The executable is unsigned.

## Verification

| Check | Result |
| --- | --- |
| Frontend | 23 tests across ten files passed, including confirmed creation passwords, clearing refused passwords and refusing Windows-bound directories. TypeScript/Vite build passed. |
| Rust | 63 library tests and all five real-worker integration tests passed. The default command ignored the shared credential-store test and four live-model tests. |
| Work-directory regressions | Copy/reopen without host credentials, wrong-password non-mutation, ciphertext identity checks, simultaneous-open refusal, tampered/short passwords, Windows-bound non-replacement and relative-location persistence passed. |
| Actual normal ZIP | Extracted and relocated; product/version and all four resource hashes/sizes matched. Native PDF extraction and mixed-PDF OCR passed using its actual executable. The locator points to `work`, and the ZIP includes no existing work directory. |
| Actual desktop UI | Six phases passed using the actual normal 0.3.0 ZIP: create, refuse a wrong password, reopen, relocate the application/work folder, restore a verified backup into another selected directory, reopen restored records. A synthetic rubric and assignment survived all successful phases. |
| Data boundaries | The normal Windows-bound database fingerprint remained unchanged. No plaintext SQLite database or password was written to the synthetic work directory. Its previous work-directory snapshot was unchanged when a different directory was restored. Each UI launch used an isolated WebView2 profile. |
| UI review | Creation desktop/600-pixel screenshots and the relocated-open screen inspected in one batch. The narrow form had no horizontal page overflow. |
| Static checks | Rust formatting, JavaScript syntax, packaging-workflow YAML and Git whitespace checks passed. |

The earlier portable verification (0.2.0) is retained as historical evidence; it described a Windows-bound ZIP and does not describe 0.3.0's work-directory behavior. No new live-model checks, hosted workflow, commit, tag, production deployment or normal installer qualification were performed for this change.

## Reproduce

```sh
pnpm package:portable
pwsh -NoProfile -File scripts/verify-portable.ps1 -ArchivePath output/portable/Corregir_0.3.0_windows_x64_portable.zip
node scripts/verify-work-directory.cjs
```

The UI script uses fictitious passwords/content, unique folders and a separate per-launch WebView2 user-data directory through Microsoft's [documented environment override](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/webview2-idl#createcorewebview2environmentwithoptions). It never opens the normal personal database; only its encrypted-file fingerprint is compared before/after. It closes its own process normally and preserves synthetic evidence.

This local run's UI results and screenshots are under `output/work-directory-qualification/e8b824f0-8d4c-4d7c-b481-90b4d8e42d29`. ZIP extraction results are under `output/portable-verification/34e7d077164b46ff9e045e3a31b75abc`.

The package workflow now runs both ZIP/resource extraction verification and the native work-directory UI verification before uploading the portable artifact. Its YAML was parsed locally; hosted execution remains unperformed. File-copy/reopening was tested on the local Windows host, rather than a second physical computer. WebView2 provisioning on a clean host, removable/FAT volumes, network shares and other platforms remain unqualified. See [portable instructions](PORTABLE-WINDOWS.md) for prerequisites and migration steps.
