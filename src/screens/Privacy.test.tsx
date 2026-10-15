import { beforeEach, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Privacy } from "./Privacy";
const { invokeMock, askMock, saveMock } = vi.hoisted(() => ({ invokeMock: vi.fn(), askMock: vi.fn(), saveMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: askMock, save: saveMock }));
beforeEach(() => {
  invokeMock.mockReset(); askMock.mockReset(); saveMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "cmd_load_preferences") return { feedback_language: "auto", retention_days: 365 };
    if (command === "cmd_retention_status") return [{ assignment_id: 4, title: "Poem", text: "Analyse", identified: 2, last_confirmed_at: "2025-06-20T10:00:00.000Z" }];
    if (command === "cmd_list_assignments") return [{ id: 4, rubric_id: 1, title: "Poem", text: "Analyse", submission_count: 2, identified: 2 }];
    if (command === "cmd_list_known_students") return [{ name: "Ana Pérez", submissions: 1, roster_entries: 1 }];
    if (command === "cmd_purge_assignment_identities") return { purged: 2, pending: 0 };
    if (command === "cmd_erase_student") return { submissions: 1, roster_entries: 1, mentions: 2 };
    if (command === "cmd_export_student_data") return 1;
    return null;
  });
});
it("shows overdue assignments and purges identities after confirmation", async () => {
  askMock.mockResolvedValue(true);
  const user = userEvent.setup(); render(<Privacy />);
  expect(await screen.findByText("1 assignment(s) exceed the retention period:")).toBeInTheDocument();
  expect(screen.getByLabelText("Retention (days)")).toHaveValue(365);
  await user.click(screen.getAllByRole("button", { name: "Purge identities" })[0]);
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_purge_assignment_identities", { assignmentId: 4 }));
  expect(await screen.findByText(/Identities removed from 2 submissions/)).toBeInTheDocument();
});
it("exports and erases one student's data", async () => {
  saveMock.mockResolvedValue("C:\\Requests\\ana.json");
  askMock.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
  const user = userEvent.setup(); render(<Privacy />);
  await waitFor(() => expect(screen.getByLabelText("Student")).toBeEnabled());
  await user.selectOptions(screen.getByLabelText("Student"), "Ana Pérez");
  expect(screen.getByText("1 submissions · 1 roster entries")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Export data (JSON)" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_export_student_data", { studentName: "Ana Pérez", destinationPath: "C:\\Requests\\ana.json" }));
  await user.click(screen.getByRole("button", { name: "Erase all data" }));
  expect(invokeMock.mock.calls.some(([command]) => command === "cmd_erase_student")).toBe(false);
  await user.click(screen.getByRole("button", { name: "Erase all data" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_erase_student", { studentName: "Ana Pérez" }));
  expect(await screen.findByText(/removed the name from 2 texts of other submissions/)).toBeInTheDocument();
});
it("saves or disables the retention period", async () => {
  const user = userEvent.setup(); render(<Privacy />);
  const days = await screen.findByLabelText("Retention (days)");
  await waitFor(() => expect(days).toBeEnabled());
  await user.clear(days); await user.type(days, "180");
  await user.click(screen.getAllByRole("button", { name: "Save" })[0]);
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_save_preferences", { preferences: { feedback_language: "auto", retention_days: 180 } }));
});
