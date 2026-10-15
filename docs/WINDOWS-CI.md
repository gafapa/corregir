# Windows packaging qualification

The packaging workflow builds the normal installer, verifies installed resources and startup, and exercises the normal portable application with synthetic work directories. It does not upload personal work directories or Windows credentials.

Installer startup waits for the encrypted workspace with a bounded 90-second deadline and checks that the process remains alive, rather than relying on a fixed sleep. It uses the Windows known-folder API for the roaming data path and records startup output from each of the two launches.

The UI harness connects to WebView2 using a loopback debugging port and an isolated browser profile. WebView2 runtime 150 introduced a restriction for elevated hosts: debugging environment overrides are ignored; machine policy or explicit environment API options are required. Microsoft describes the behavior in [WebView2Feedback issue 5645](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5645).

GitHub-hosted Windows runners therefore receive temporary machine policies scoped to `Corregir.exe` for the debugging arguments and synthetic browser profile. The harness refuses to overwrite existing values and removes its values after each launch and during final cleanup. This configuration is enabled only when both `GITHUB_ACTIONS=true` and `RUNNER_ENVIRONMENT=github-hosted`. Local workstations and self-hosted runners retain their existing policies. The installed-application qualification (`qualify-desktop.cjs`, workflow `qualify.yml`) applies the same temporary debugging-arguments policy, scoped to the qualification executable `corregir.exe`. The released application's code, configuration and security settings are unchanged.

The harness chooses a free loopback port, retains all password, relocation and restore assertions, and records launch diagnostics when a connection fails. The workflow retains synthetic screenshots and diagnostic summaries for seven days. The complete work directory, its encrypted key and backup files are excluded from uploaded diagnostics.

Publication requires successful verification and Windows packaging runs for the release commit. The uploaded release binaries are checked against their SHA-256 digests, and public downloads are verified after publication.
