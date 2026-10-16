import { beforeEach, it, expect, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Submissions } from "./Submissions";
const { invokeMock, openMock, askMock } = vi.hoisted(() => ({ invokeMock: vi.fn(), openMock: vi.fn(), askMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ Channel: class { onmessage: unknown; }, invoke: invokeMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openMock, save: vi.fn(), ask: askMock }));
const rubrics = [
  { id: 1, title: "First rubric", version: 1, criteria: [{ id: 10, code: "C1", description: "First criterion", score_max: 10, sort_order: 0 }] },
  { id: 2, title: "Second rubric", version: 1, criteria: [{ id: 20, code: "C2", description: "Second criterion", score_max: 10, sort_order: 0 }] },
];
const submissions = [
  { id: 7, assignment_id: 9, status_pipeline: "redacted", student_name: "Student", text_ocr: "text", method_ocr: "text_native", has_evidence: false },
  { id: 8, assignment_id: 9, status_pipeline: "ocr_complete", student_name: null, source_name: "brais-souto.pdf", text_ocr: "other", method_ocr: "ocr_local" },
];
beforeEach(() => {
  invokeMock.mockReset(); openMock.mockReset(); askMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "cmd_list_rubrics") return rubrics;
    if (command === "cmd_list_assignments") return [{ id: 9, rubric_id: 2, title: "Saved assignment", text: "Instructions", submission_count: 2, class_id: 4 }];
    if (command === "cmd_list_classes") return [{ id: 4, name: "3B", students: [{ id: 1, full_name: "Brais Souto" }, { id: 2, full_name: "Ana Pérez" }] }];
    if (command === "cmd_list_submissions") return submissions;
    if (command === "cmd_load_grading_state") return { revision: 0, criteria: [], feedback: null };
    if (command === "cmd_detect_identifiers") return [];
    if (command === "cmd_import_submission") return 11;
    return null;
  });
});
it("reopens saved work with its actual rubric and selects the first pending submission", async () => {
  localStorage.setItem("corregir.activeAssignmentId", "9");
  render(<Submissions />);
  expect(await screen.findByText("Second criterion", { exact: false })).toBeInTheDocument();
  expect(screen.queryByText("First criterion", { exact: false })).not.toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Saved assignment" })).toBeInTheDocument();
  expect(screen.getByText(/Class 3B \(2 students\)/)).toBeInTheDocument();
});
it("filters the submission list by text and status", async () => {
  const user = userEvent.setup(); render(<Submissions />);
  const search = await screen.findByLabelText("Search submissions");
  await user.type(search, "does not exist");
  expect(screen.getByText("No submissions match these filters.", { exact: false })).toBeInTheDocument();
  await user.clear(search); await user.type(search, "brais");
  expect(screen.getByText("Showing 1 of 2 submissions")).toBeInTheDocument();
  await user.clear(search);
  await user.selectOptions(screen.getByLabelText("Submission status"), "grade_confirmed");
  expect(screen.getByText("Showing 0 of 2 submissions")).toBeInTheDocument();
  await user.selectOptions(screen.getByLabelText("Submission status"), "redacted");
  expect(screen.getByText("Showing 1 of 2 submissions")).toBeInTheDocument();
});
it("selects a submission from the list and suggests its student from the file name", async () => {
  const user = userEvent.setup(); render(<Submissions />);
  const list = await screen.findByRole("complementary", { name: "Submissions in this assignment" });
  await user.click(within(list).getByRole("button", { name: /brais-souto\.pdf/ }));
  expect(await screen.findByRole("heading", { name: "brais-souto.pdf" })).toBeInTheDocument();
  await waitFor(() => expect(screen.getByLabelText("Student")).toHaveValue("1"));
  expect(screen.getByText(/Suggested from the file name/)).toBeInTheDocument();
  expect(invokeMock).toHaveBeenCalledWith("cmd_detect_identifiers", { submissionId: 8, roster: ["Brais Souto", "Ana Pérez"] });
});
it("imports several documents in sequence and reports failures without stopping", async () => {
  openMock.mockResolvedValue(["C:\\Class\\ana.pdf", "C:\\Class\\broken.pdf", "C:\\Class\\uxia.pdf"]);
  invokeMock.mockImplementation(async (command: string, args: { filePath?: string }) => {
    if (command === "cmd_import_submission") { if (args.filePath?.endsWith("broken.pdf")) throw new Error("document exceeds the file, page, image, or text safety limits"); return 12; }
    if (command === "cmd_list_rubrics") return rubrics;
    if (command === "cmd_list_assignments") return [{ id: 9, rubric_id: 2, title: "Saved assignment", text: "Instructions", submission_count: 2 }];
    if (command === "cmd_list_submissions") return submissions;
    if (command === "cmd_list_classes") return [];
    if (command === "cmd_detect_identifiers") return [];
    if (command === "cmd_load_grading_state") return { revision: 0, criteria: [], feedback: null };
    return null;
  });
  const user = userEvent.setup(); render(<Submissions />);
  await user.click(await screen.findByRole("button", { name: "Import documents" }));
  expect(await screen.findByText("Imported and saved 2 of 3 documents.")).toBeInTheDocument();
  expect(screen.getByRole("alert")).toHaveTextContent("broken.pdf: document exceeds");
  const imports = invokeMock.mock.calls.filter(([command]) => command === "cmd_import_submission");
  expect(imports.map(([, args]) => (args as { filePath: string }).filePath)).toEqual(["C:\\Class\\ana.pdf", "C:\\Class\\broken.pdf", "C:\\Class\\uxia.pdf"]);
});
it("deletes a submission only after confirmation", async () => {
  askMock.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
  const user = userEvent.setup(); render(<Submissions />);
  const remove = await screen.findByRole("button", { name: "Delete submission" });
  await user.click(remove);
  expect(invokeMock.mock.calls.some(([command]) => command === "cmd_delete_submission")).toBe(false);
  await user.click(remove);
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_delete_submission", { submissionId: 7 }));
});
