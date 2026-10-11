import { it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Submissions } from "./Submissions";
vi.mock("@tauri-apps/api/core", () => ({
  Channel: class {},
  invoke: vi.fn(async (command: string) => {
    if (command === "cmd_list_rubrics") return [
      { id: 1, title: "First rubric", criteria: [{ id: 10, code: "C1", description: "First criterion", score_max: 10, sort_order: 0 }] },
      { id: 2, title: "Second rubric", criteria: [{ id: 20, code: "C2", description: "Second criterion", score_max: 10, sort_order: 0 }] },
    ];
    if (command === "cmd_list_assignments") return [{ id: 9, rubric_id: 2, text: "Saved assignment", submission_count: 1 }];
    if (command === "cmd_list_submissions") return [{ id: 7, assignment_id: 9, status_pipeline: "redacted", student_name: "Student", text_ocr: "text", method_ocr: "text_native" }];
    if (command === "cmd_load_grading_state") return { revision: 0, criteria: [], feedback: null };
    throw new Error(`Unexpected command: ${command}`);
  }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
it("reopens saved work and keeps its actual rubric when preparing another assignment", async () => {
  localStorage.setItem("corregir.activeAssignmentId", "9");
  const user = userEvent.setup();
  render(<Submissions />);
  expect(await screen.findByText("Second criterion", { exact: false })).toBeInTheDocument();
  await waitFor(() => expect(screen.getByLabelText("Rubric:")).toBeEnabled());
  await user.selectOptions(screen.getByLabelText("Rubric:"), "1");
  expect(screen.getByText("Second criterion", { exact: false })).toBeInTheDocument();
  expect(screen.queryByText("First criterion", { exact: false })).not.toBeInTheDocument();
});
it("filters saved submissions by text and status without changing their records", async () => {
  const user = userEvent.setup(); render(<Submissions />);
  const search = await screen.findByLabelText("Search submissions");
  await user.type(search,"does not exist");
  expect(screen.getByText("No submissions match these filters.",{exact:false})).toBeInTheDocument();
  await user.clear(search); await user.type(search,"Student");
  expect(screen.getByText("Showing 1 of 1 submissions")).toBeInTheDocument();
  await user.selectOptions(screen.getByLabelText("Submission status"),"grade_confirmed");
  expect(screen.getByText("Showing 0 of 1 submissions")).toBeInTheDocument();
  await user.selectOptions(screen.getByLabelText("Submission status"),"redacted");
  expect(screen.getByText("Showing 1 of 1 submissions")).toBeInTheDocument();
});
