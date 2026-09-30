# Interface design

The desktop workspace retains its blue accent, system typography, English interface and teacher-reviewed grading workflow. The refinement improves information hierarchy and consistency without changing the inference, redaction or grading contracts.

## Shared visual language

`src/App.css` defines semantic tokens for text, surfaces, borders, selection, focus, primary actions, success, warnings and errors. Light and dark variants follow the operating system preference. Action buttons have a minimum height of 44 pixels. The interface includes visible keyboard focus and reduced-motion support. All fonts and visual assets work offline.

The header separates the product name from the synthetic-data environment indicator. Navigation identifies the current section with `aria-current`, weight, color and an underline. A keyboard skip link moves directly to the workspace.

## Task layout

- Submissions separates saved assignments from assignment creation. The saved-work area shows the active instructions, rubric and submission count, alongside import/export actions. Narrow windows stack the two areas.
- Submission headings distinguish extracted documents awaiting redaction, documents ready for grading and confirmed grades. Evidence, teacher scores, comments and generated feedback have separate visual groups.
- The grading action emphasis follows the saved state: saving is primary while edits are pending; confirmation is primary after saving. Existing confirmation requirements remain in force.
- Rubrics and audit events include explanatory empty/loading states. Audit tables scroll within their own keyboard-accessible region.
- Export successes appear inline, replacing blocking browser alerts. Shared alert colors work in both themes. Redaction inputs are disabled while detection or confirmation is running.
- Diagnostics aligns model selection with its connection action and uses the same feedback styles as the rest of the application.

## Verification

A bounded browser inspection used injected synthetic IPC fixtures, not actual student records or native backend calls. Nine screenshots covered all four sections, grading and redaction, light/dark themes, the standard 1100 × 750 desktop viewport, a 680-pixel window and a 390-pixel stress case. No inspected page overflowed horizontally, no buttons fell below 44 pixels and no runtime exceptions were recorded. The diagnostic error and confirmed-grade export states were exercised. Screenshots and inspection output remain local under the ignored `output/interface-previews` directory.

The Impeccable mechanical detector reported no findings for the changed interface files. Existing workflow regression tests, TypeScript compilation, the production frontend build and the backend checks run in [GitHub Actions](https://github.com/gafapa/corregir/actions/workflows/ci.yml). The visual fixture check does not substitute for native file-dialog or real-model integration testing.

The confirmation inspection verified keyboard activation of the skip link and focus transfer to the workspace. Measured light-theme contrast ratios were 15.04:1 for headings and labels, 5.88:1 for descriptive text and 7.22:1 for primary button text. The [hosted verification of the interface change](https://github.com/gafapa/corregir/actions/runs/36750016652), commit `bb013b32a7d31c8b6ba16db5033f94026275a258`, completed successfully: 5 frontend tests and 38 backend tests passed, frontend compilation and resource verification passed, and both dependency audits passed with the two previously documented GTK warnings remaining visible.

Vite excludes temporary browser profiles and verification output from file watching to prevent Windows file-lock failures during development.
