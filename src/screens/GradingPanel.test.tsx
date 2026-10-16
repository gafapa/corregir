import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { GradingPanel } from "./GradingPanel";
const { invokeMock, saveMock } = vi.hoisted(() => ({ invokeMock: vi.fn(), saveMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock, Channel: class { onmessage: unknown; } }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: saveMock }));
const submission = { id: 3, assignment_id: 1, text_ocr: "answer", method_ocr: "text_native", status_pipeline: "redacted", student_name: "Test Student" };
const criteria = [{ id: 4, code: "C1", description: "Criterion", score_max: 10, sort_order: 0, levels: [{ label: "Complete", score: 10, description: "" }, { label: "Partial", score: 5, description: "" }] }];
beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "cmd_load_grading_state") return { revision: 7, criteria: [{ criterion_id: "C1", score: 4, comment_teacher: "Stored comment", evidence_textual: ["answer"] }], feedback: { comment_feedback: "Stored feedback", inconsistencies: [] }, feedback_final: null, feedback_ai_assisted: false };
    if (command === "cmd_save_grade_tentative") return 8;
    if (command === "cmd_confirm_grade") return 4;
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
    expect(screen.getByText("Total: 4 / 10 points · Grade: 4 out of 10")).toBeInTheDocument();
    const confirm = screen.getByRole("button", { name: /Confirm grade/ });
    expect(confirm).toBeEnabled();
    await user.clear(score);
    await user.type(score, "6");
    expect(confirm).toBeDisabled();
    expect(screen.getByRole("button", { name: /Generate feedback/ })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: /Save my assessment/ }));
    await waitFor(() => expect(confirm).toBeEnabled());
    expect(invokeMock).toHaveBeenCalledWith("cmd_save_grade_tentative", { submissionId: 3, expectedRevision: 7, assessments: [{ criterion_id: "C1", score: 6, comment_teacher: "Stored comment" }] });
  });
  it("fills a score from a performance level", async () => {
    const user = userEvent.setup();
    render(<GradingPanel submission={submission} criteria={criteria} />);
    const score = await screen.findByRole("spinbutton");
    await waitFor(() => expect(score).toHaveValue(4));
    await user.click(screen.getByRole("button", { name: "Partial · 5" }));
    expect(score).toHaveValue(5);
    expect(screen.getByRole("button", { name: "Partial · 5" })).toHaveAttribute("aria-pressed", "true");
  });
  it("shows the AI draft for editing and saves the teacher's text before confirming", async () => {
    const user = userEvent.setup();
    render(<GradingPanel submission={submission} criteria={criteria} />);
    const feedback = await screen.findByLabelText(/Feedback for the student/);
    await waitFor(() => expect(feedback).toHaveValue("Stored feedback"));
    await user.clear(feedback);
    await user.type(feedback, "Well argued; cite the poem.");
    await user.click(screen.getByRole("button", { name: /Confirm grade/ }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_confirm_grade", { submissionId: 3, expectedRevision: 7 }));
    expect(invokeMock).toHaveBeenCalledWith("cmd_save_feedback", { submissionId: 3, text: "Well argued; cite the poem.", aiAssisted: true });
    const saveOrder = invokeMock.mock.calls.findIndex(([command]) => command === "cmd_save_feedback");
    const confirmOrder = invokeMock.mock.calls.findIndex(([command]) => command === "cmd_confirm_grade");
    expect(saveOrder).toBeLessThan(confirmOrder);
  });
  it("reports a rejected export dialog", async () => {
    saveMock.mockRejectedValue(new Error("Dialog failed"));
    const user = userEvent.setup();
    render(<GradingPanel submission={{ ...submission, status_pipeline: "grade_confirmed" }} criteria={criteria} />);
    await user.click(screen.getByRole("button", { name: /Export feedback/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Dialog failed");
  });
  it("reopens a confirmed grade only with a reason", async () => {
    const user = userEvent.setup();
    const changed = vi.fn();
    render(<GradingPanel submission={{ ...submission, status_pipeline: "grade_confirmed" }} criteria={criteria} onGradeConfirmed={changed} />);
    await user.click(screen.getByRole("button", { name: "Reopen grade" }));
    const reopen = screen.getByRole("button", { name: "Reopen for editing" });
    expect(reopen).toBeDisabled();
    await user.type(screen.getByLabelText(/Reason for reopening/), "Sum error");
    await user.click(reopen);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_reopen_grade", { submissionId: 3, reason: "Sum error" }));
    expect(changed).toHaveBeenCalledOnce();
  });
  it("confirms only the revision that was loaded for review", async () => {
    invokeMock.mockImplementation(async (command: string) => command === "cmd_load_grading_state" ? { revision: 7, criteria: [{ criterion_id: "C1", score: 4, comment_teacher: null, evidence_textual: [] }], feedback: null } : 4);
    const user = userEvent.setup();
    render(<GradingPanel submission={submission} criteria={criteria} />);
    const confirm = screen.getByRole("button", { name: /Confirm grade/ });
    await waitFor(() => expect(confirm).toBeEnabled());
    await user.click(confirm);
    expect(invokeMock).toHaveBeenCalledWith("cmd_confirm_grade", { submissionId: 3, expectedRevision: 7 });
    expect(invokeMock.mock.calls.some(([command]) => command === "cmd_save_feedback")).toBe(false);
  });
  it("disables confirmation and feedback when reloading the saved assessment fails", async () => {
    const user = userEvent.setup();
    render(<GradingPanel submission={submission} criteria={criteria} />);
    const confirm = screen.getByRole("button", { name: /Confirm grade/ });
    await waitFor(() => expect(confirm).toBeEnabled());
    invokeMock.mockRejectedValueOnce(new Error("Reload failed"));
    await user.click(screen.getByRole("button", { name: /Reload saved assessment/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Reload failed");
    expect(confirm).toBeDisabled();
    expect(screen.getByRole("button", { name: /Generate feedback/ })).toBeDisabled();
  });
  it("shows the queue stage, discarded quotes and cancels the active inference request", async () => {
    const user = userEvent.setup();
    let rejectRequest: (error: Error) => void = () => {};
    const original = invokeMock.getMockImplementation();
    invokeMock.mockImplementation((command, args) => {
      if (command === "cmd_request_evidence") return new Promise((_resolve, reject) => { rejectRequest = reject; });
      if (command === "cmd_cancel_inference") { rejectRequest(new Error("AI request cancelled")); return Promise.resolve(); }
      return original?.(command, args);
    });
    render(<GradingPanel submission={submission} criteria={criteria} />);
    const find = screen.getByRole("button", { name: /Find evidence/ });
    await waitFor(() => expect(find).toBeEnabled());
    await user.click(find);
    expect(await screen.findByText("Waiting for the local model…")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel AI request" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("AI request cancelled");
    expect(invokeMock).toHaveBeenCalledWith("cmd_cancel_inference", { requestId: expect.any(String) });
    invokeMock.mockImplementation((command, args) => command === "cmd_request_evidence" ? Promise.resolve({ evidence: [{ criterion_id: "C1", evidence_textual: ["new quote"] }], discarded_quotes: 2 }) : original?.(command, args));
    await user.click(screen.getByRole("button", { name: /Find evidence/ }));
    expect(await screen.findByText('"new quote"')).toBeInTheDocument();
    expect(screen.getByText(/2 quote\(s\) proposed by the model were not found/)).toBeInTheDocument();
  });
});
