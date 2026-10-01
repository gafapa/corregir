import { beforeEach, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { BackupTools } from "./BackupTools";
const { invokeMock, openMock, saveMock } = vi.hoisted(() => ({ invokeMock: vi.fn(), openMock: vi.fn(), saveMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openMock, save: saveMock }));
beforeEach(() => { invokeMock.mockReset();openMock.mockReset();saveMock.mockReset(); });
it("requires matching passwords and clears them after creating an encrypted backup", async () => {
  saveMock.mockResolvedValue("C:\\Synthetic\\backup.corregirbackup"); invokeMock.mockResolvedValue(null);
  const user = userEvent.setup(); render(<BackupTools />);
  const password=screen.getByLabelText("Backup password"); const confirmation=screen.getByLabelText("Repeat password to create a backup");
  await user.type(password,"synthetic-password");expect(screen.getByRole("button",{name:"Create encrypted backup"})).toBeDisabled();
  await user.type(confirmation,"synthetic-password");await user.click(screen.getByRole("button",{name:"Create encrypted backup"}));
  await waitFor(()=>expect(invokeMock).toHaveBeenCalledWith("cmd_export_backup",{destinationPath:"C:\\Synthetic\\backup.corregirbackup",password:"synthetic-password"}));
  await waitFor(()=>expect(password).toHaveValue(""));expect(confirmation).toHaveValue("");
});
it("reports a refused restore without replacing the screen or retaining the password", async () => {
  openMock.mockResolvedValue("C:\\Synthetic\\backup.corregirbackup");invokeMock.mockRejectedValue(new Error("restore requires an empty workspace"));
  const user=userEvent.setup();render(<BackupTools />);
  await user.click(screen.getByRole("button",{name:"Select backup"}));await user.type(screen.getByLabelText("Backup password"),"synthetic-password");
  await user.click(screen.getByRole("button",{name:"Restore selected backup"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("empty workspace");await waitFor(()=>expect(screen.getByLabelText("Backup password")).toHaveValue(""));
});
