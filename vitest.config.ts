import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
export default defineConfig({
  plugins: [react()],
  test: { pool: "threads", maxWorkers: 1, testTimeout: 15000, environment: "jsdom", setupFiles: ["./src/test/setup.ts"], restoreMocks: true },
});
