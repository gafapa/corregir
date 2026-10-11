# Windows portable package validation — 2026-10-10

## Artifact

- Normal application: Corregir 0.2.0, Windows x64.
- Archive: `output/portable/Corregir_0.2.0_windows_x64_portable.zip`.
- Size: 21,829,228 bytes.
- SHA-256: `09d6e24e1be277323d8955c1fd4bfb3665bb570332d7e014d375053078571866`.
- The ZIP contains the normal executable, configured runtime resources, third-party licenses and an English portable README. It contains no workspace, user credentials, installer or qualification executable.
- The executable remains unsigned.

## Checks performed

`package-portable.ps1 -SkipBuild` reused the previously validated normal 0.2.0 release executable, checked its product/version metadata, copied every resource mapping from the Tauri configuration and verified all four manifest hashes/sizes before creating the ZIP and SHA-256 sidecar.

`verify-portable.ps1` extracted that actual ZIP into an isolated directory, moved its application folder to a different path containing spaces, checked the product/version and all four resources again, then ran the actual normal executable's document-worker entry point from an unrelated working directory. Native extraction included the expected answer without OCR; mixed-PDF extraction included the scanned answer and reported OCR. Both returned the exact input-file SHA-256. Result evidence is under `output/portable-verification/00073d79834d46f29b6b46c097584b65`.

A separate GUI check used qualification version 0.1.2 and its synthetic encrypted workspace. The fixture application/resources were copied out of their installation directory; the fixture installation was uninstalled before launching the relocated executable. Native UI/IPC checked the confirmed assessment (score 8, revision 1, preserved feedback), selected and rendered the matching original through the restricted worker, and exercised zoom and the narrow layout. It closed normally. Evidence is under `output/portable-gui-verification`; its encrypted test profile was archived afterwards. The normal personal workspace and credential were not opened by these checks.

The GUI check uses the qualification-only picker substitute, while the extraction check uses the actual normal ZIP executable. This distinction is retained: it is not a claim that a normal production GUI session was automated against personal data. No remote workflow or release publication was performed.

## Reproduce

```sh
pnpm package:portable
pwsh -NoProfile -File scripts/verify-portable.ps1 -ArchivePath output/portable/Corregir_0.2.0_windows_x64_portable.zip
```

The Windows packaging workflow now creates, verifies and uploads the ZIP/hash alongside the installer. Its YAML was parsed locally; hosted execution remains unperformed.

## Scope

This is a no-install application package. Assessments and the encryption credential continue to use the Windows user profile. Cross-computer workspace transfer requires password-protected backup export/restore, rather than copying the application folder. See [portable instructions](PORTABLE-WINDOWS.md).

The local Windows host already has WebView2. The package does not include that runtime, Ollama or a language model. Clean-machine runtime provisioning, other CPU architectures, removable/FAT volumes, network shares and folder-only encrypted workspaces were not qualified in this run.
