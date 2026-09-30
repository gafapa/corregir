import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { GradingPanel } from "./GradingPanel";
const { invokeMock, saveMock } = vi.hoisted(() => ({ invokeMock: vi.fn(), saveMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: saveMock }));
const submission = { id: 3, assignment_id: 1, text_ocr: "answer", method_ocr: "text_native", status_pipeline: "redacted", student_name: "Test Student" };
const criteria = [{ id: 4, code: "C1", description: "Criterion", score_max: 10, sort_order: 0 }];
beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "cmd_load_grading_state") return { revision: 7, criteria: [{ criterion_id: "C1", score: 4, comment_teacher: "Stored comment", evidence_textual: ["answer"] }], feedback: { comment_feedback: "Stored feedback", inconsistencies: [] } };
    if (command === "cmd_save_grade_tentative") return 8;
    return null;
  });
});
describe("saved assessment", () => {
  it("restores evidence and scores and requires saving edits against the revision", async () => {
    const user = userEvent.setup();
    render(<GradingPanel submission={submission} criteria={criteria} />);
    const score = await screen.findByRole("spinbutton");
    await waitFor(() => expect(score).toHaveValue(4));
    expect(screen.getByText('"answer"')).toBeInTheDocument();
    const confirm = screen.getByRole("button", { name: /Confirm grade/ });
    expect(confirm).toBeEnabled();
    await user.clear(score);
    await user.type(score, "6");
    expect(confirm).toBeDisabled();
    expect(screen.getByRole("button", { name: /Generate feedback/ })).toBeDisabled();
    expect(screen.queryByText("Stored feedback")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Save my assessment/ }));
    await waitFor(() => expect(confirm).toBeEnabled());
    expect(invokeMock).toHaveBeenCalledWith("cmd_save_grade_tentative", { submissionId: 3, expectedRevision: 7, assessments: [{ criterion_id: "C1", score: 6, comment_teacher: "Stored comment" }] });
  });
  it("reports a rejected export dialog", async () => {
    saveMock.mockRejectedValue(new Error("Dialog failed"));
    const user = userEvent.setup();
    render(<GradingPanel submission={{ ...submission, status_pipeline: "grade_confirmed" }} criteria={criteria} />);
    await user.click(screen.getByRole("button", { name: /Export feedback/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Dialog failed");
  });
});
