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
it("checks recovery without restoring and reports changed backup files", async () => {
  openMock.mockResolvedValue("C:\\Synthetic\\backup.corregirbackup");
  invokeMock.mockImplementation(async command => command === "cmd_backup_status" ? { last_created_at: null, last_verified_at: null, needs_backup: true, integrity_ok: false } : null);
  const user = userEvent.setup(); render(<BackupTools />);
  expect(await screen.findByText(/The recorded backup is missing or has changed/)).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Select backup" }));
  await user.type(screen.getByLabelText("Backup password"), "synthetic-password");
  await user.click(screen.getByRole("button", { name: "Check recovery without restoring" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_verify_backup", { sourcePath: "C:\\Synthetic\\backup.corregirbackup", password: "synthetic-password" }));
  expect(invokeMock.mock.calls.some(([command]) => command === "cmd_restore_backup")).toBe(false);
  await waitFor(() => expect(screen.getByLabelText("Backup password")).toHaveValue(""));
});
