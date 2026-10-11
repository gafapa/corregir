import { it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DocumentReview } from "./DocumentReview";
import { invoke } from "@tauri-apps/api/core";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
it("saves text against the reviewed version and blocks redaction while corrections are pending", async () => {
  const user = userEvent.setup();
  const draft = vi.fn(); const saved = vi.fn(async () => {});
  render(<DocumentReview submission={{ id: 8, assignment_id: 2, text_ocr: "speod", status_pipeline: "ocr_complete", student_name: null, method_ocr: "ocr_local" }} onSaved={saved} onDraftChange={draft} />);
  await user.click(screen.getByText("Compare original and extracted text"));
  const input = screen.getByLabelText("Extracted text (all pages)");
  await user.clear(input); await user.type(input,"speed");
  expect(draft).toHaveBeenLastCalledWith(true);
  await user.click(screen.getByRole("button",{name:"Save corrected text"}));
  expect(invoke).toHaveBeenCalledWith("cmd_save_extracted_text",{submissionId:8,expectedText:"speod",text:"speed"});
  expect(saved).toHaveBeenCalledOnce();
});
it("keeps reviewed text read-only after redaction", async () => {
  render(<DocumentReview submission={{ id: 9, assignment_id: 2, text_ocr: "reviewed", status_pipeline: "redacted", student_name: "Synthetic", method_ocr: "text_native" }} onSaved={vi.fn()} onDraftChange={vi.fn()} />);
  expect(screen.getByLabelText("Extracted text (all pages)")).toHaveAttribute("readonly");
  expect(screen.queryByRole("button",{name:"Save corrected text"})).not.toBeInTheDocument();
});
