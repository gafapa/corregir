import { beforeEach, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Rubrics } from "./Rubrics";
const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => command === "cmd_list_rubrics" ? [{ id: 2, title: "Stored rubric", subject: "Science", grade_level: "Year 9", version: 1, criteria: [{ id: 4, code: "C1", description: "Stored criterion", score_max: 10, sort_order: 0 }] }] : 3);
});
it("creates a custom rubric with its teacher-defined criteria", async () => {
  const user = userEvent.setup(); render(<Rubrics />);
  await waitFor(() => expect(screen.getByLabelText("Title")).toBeEnabled());
  await user.type(screen.getByLabelText("Title"), "My assessment"); await user.type(screen.getByLabelText("Subject"), "History"); await user.type(screen.getByLabelText("Year group"), "Year 8");
  await user.type(screen.getByLabelText("Description 1"), "Explains the historical context.");
  await user.click(screen.getByRole("button", { name: "Save rubric" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_create_rubric", { rubric: { title: "My assessment", subject: "History", grade_level: "Year 8", criteria: [{ code: "C1", description: "Explains the historical context.", score_max: 10 }] } }));
});
it("saves edits as a new version and never overwrites the source rubric", async () => {
  const user = userEvent.setup(); render(<Rubrics />);
  await user.click(await screen.findByRole("button", { name: /Edit Stored rubric/ }));
  expect(screen.getByLabelText("Title")).toHaveFocus();
  await user.clear(screen.getByLabelText("Maximum score 1")); await user.type(screen.getByLabelText("Maximum score 1"), "5");
  await user.click(screen.getByRole("button", { name: "Save as new version" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_create_rubric_version", { rubricId: 2, expectedVersion: 1, rubric: { title: "Stored rubric", subject: "Science", grade_level: "Year 9", criteria: [{ code: "C1", description: "Stored criterion", score_max: 5 }] } }));
});
it("refuses duplicate criterion codes before a backend write", async () => {
  const user = userEvent.setup(); render(<Rubrics />);
  await user.click(await screen.findByRole("button", { name: /Edit Stored rubric/ }));
  await user.click(screen.getByRole("button", { name: "Add criterion" }));
  await user.clear(screen.getByLabelText("Code 2")); await user.type(screen.getByLabelText("Code 2"), "C1"); await user.type(screen.getByLabelText("Description 2"), "Another criterion");
  await user.click(screen.getByRole("button", { name: "Save as new version" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("unique code");
  expect(invokeMock).not.toHaveBeenCalledWith("cmd_create_rubric_version", expect.anything());
});
