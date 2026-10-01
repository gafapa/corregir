import { expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Diagnostics } from "./Diagnostics";
const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn(async (command: string) => command === "cmd_load_ollama_settings" ? { url: "http://localhost:11435", model: "llama3.2:3b" } : "Connected") }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
it("restores configuration and saves the selected model used for subsequent correction", async () => {
  const user = userEvent.setup(); render(<Diagnostics />);
  await waitFor(() => expect(screen.getByLabelText("Model:")).toBeEnabled());
  expect(screen.getByLabelText("Server URL")).toHaveValue("http://localhost:11435");
  await user.clear(screen.getByLabelText("Model:")); await user.type(screen.getByLabelText("Model:"), "qwen3:8b");
  await user.click(screen.getByRole("button", { name: "Save inference settings" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_save_ollama_settings", { settings: { url: "http://localhost:11435", model: "qwen3:8b" } }));
  await user.click(screen.getByRole("button", { name: "Test AI connection" }));
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("cmd_test_ai_connection", { url: "http://localhost:11435", model: "qwen3:8b" }));
});
