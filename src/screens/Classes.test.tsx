import { beforeEach, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Classes, parseRoster } from "./Classes";
import { suggestStudent } from "./RedactionPanel";
const { invokeMock, askMock } = vi.hoisted(() => ({ invokeMock: vi.fn(), askMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: askMock }));
beforeEach(() => {
  invokeMock.mockReset(); askMock.mockReset();
  invokeMock.mockImplementation(async (command: string) => command === "cmd_list_classes" ? [{ id: 3, name: "3B", students: [{ id: 1, full_name: "Ana Pérez" }] }] : 3);
});
it("parses pasted spreadsheet columns", () => {
  expect(parseRoster("Ana Pérez\t3B\r\n\n  Brais   Souto  \nana pérez\n")).toEqual(["Ana Pérez", "Brais Souto"]);
});
it("suggests a student only when the match is unambiguous", () => {
  const roster = [{ id: 1, full_name: "Ana Pérez Souto" }, { id: 2, full_name: "Brais Souto Rey" }];
  expect(suggestStudent(roster, ["perez_ana_tarefa3.pdf"])?.id).toBe(1);
  expect(suggestStudent(roster, ["souto.pdf"])).toBeUndefined();
  expect(suggestStudent(roster, ["scan-0001.pdf"])).toBeUndefined();
});
it("creates a class from a pasted roster and edits an existing one", async () => {
  const user = userEvent.setup(); render(<Classes />);
  await waitFor(() => expect(screen.getByLabelText("Class name")).toBeEnabled());
  await user.type(screen.getByLabelText("Class name"), "4A");
  await user.type(screen.getByLabelText(/Students \(one full name per line\)/), "Uxía Rey{enter}Iago Lema");
  expect(screen.getByText("2 names. Duplicates and blank lines are ignored.")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_save_class", { classId: null, name: "4A", students: ["Uxía Rey", "Iago Lema"] }));
  await user.click(screen.getByRole("button", { name: "Edit class 3B" }));
  expect(screen.getByLabelText(/Students/)).toHaveValue("Ana Pérez");
  await user.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_save_class", { classId: 3, name: "3B", students: ["Ana Pérez"] }));
});
it("deletes a class only after confirmation", async () => {
  askMock.mockResolvedValue(true);
  const user = userEvent.setup(); render(<Classes />);
  await user.click(await screen.findByRole("button", { name: "Delete class 3B" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_delete_class", { classId: 3 }));
});
