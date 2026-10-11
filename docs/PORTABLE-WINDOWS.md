# Corregir for Windows — no-install ZIP

## Run

1. Extract the entire ZIP into a local folder. Do not run the application inside the ZIP.
2. Keep `Corregir.exe` together with its `pdfium`, `models`, `fonts`, manifest and license files.
3. Run `Corregir.exe`. Installation, administrator privileges and an application shortcut are not required.
4. Choose a work directory or keep the default `work` folder beside the executable. Create and confirm its password (at least 12 characters); existing portable directories ask for their password instead.

This package targets Windows x64 and requires Microsoft Edge WebView2 Runtime on the computer. It does not install or bundle WebView2, Ollama or the language model. Local OCR resources are included; AI assistance requires a separately available local Ollama server/model. See [Tauri's Windows runtime requirements](https://v2.tauri.app/distribute/windows-installer/#webview2-installation-options).

The executable is unsigned. The application remains the Phase A synthetic-data prototype.

## Workspace and moving to another computer

Starting with version 0.3.0, the portable ZIP includes `corregir-portable.json`, which enables an explicit, password-protected work directory. The default location is relative (`work`), so it follows the application when its complete folder moves. A different directory can be selected at startup and is remembered after successful opening. Close the application before choosing another directory or copying its files.

The directory contains `corregir.sqlite.enc` and `corregir.workspace-key`. The database uses a random encryption key, wrapped with the work-directory password using Argon2id and authenticated AES-256-GCM. No password or unprotected key is written. Portable opening does not use the Windows credential store. The workspace lock prevents simultaneous writers, and an incorrect password does not replace data. The ZIP contains no existing workspace or personal data.

To move work, close Corregir and copy the complete work directory, including its protected key file. On the destination computer, choose the copied directory and enter its password. For the default relative location, copying the complete application folder also carries `work`. Keep the password separately. Original documents are not copied into the workspace and must be transferred separately if original-page comparison is needed.

Continue to export and verify password-protected backups to a location outside the application/work directories. A lost password or missing/corrupt key requires restoring a verified backup into a new empty work directory. A password cannot be reset to decrypt an existing workspace.

## Existing installed or 0.2.0 ZIP workspaces

The installed application, and the earlier 0.2.0 ZIP, retain their Windows-bound workspace in `%APPDATA%\com.corregir.desktop` with its credential in Windows. They are not converted automatically. Export and verify a password-protected backup in that version, create a new portable work directory in 0.3.0, then restore the backup using backup tools. Selecting a Windows-bound or legacy plaintext workspace directly is refused without replacing its files.

WebView2 still maintains its browser/runtime profile in Windows application data. This work-directory feature does not make every runtime cache or system dependency folder-local. WebView2, Ollama and the language model remain separate prerequisites.

## Update

Create and verify a password-protected backup, close Corregir and extract the new release into a new folder. Keep the complete resource set from that release together. Copy the complete `work` directory to the new application folder, or choose your existing external work directory at startup. Keep its protected key file; moving only the database is insufficient.

## Rebuild

From the project root on Windows with PowerShell 7:

```sh
pnpm package:portable
```

The script builds the normal release executable without an installer, copies the configured resources and licenses, checks every manifest resource hash/size and writes the portable locator, ZIP and SHA-256 sidecar under `output/portable`. `pwsh -NoProfile -File scripts/package-portable.ps1 -SkipBuild` reuses a matching normal release executable. No qualification executable or personal data is included.
