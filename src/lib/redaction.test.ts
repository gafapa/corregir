import { describe, it, expect } from "vitest";
import { findManualSpans } from "./redaction";
describe("manual redaction", () => {
  it("redacts repeated accented names after supplementary Unicode characters", () => {
    const text = "😀 José respondió. José terminó.";
    const spans = findManualSpans(text, "José");
    let bytes = new TextEncoder().encode(text);
    for (const [start, end] of [...spans].reverse()) {
      const replacement = new TextEncoder().encode("[alias]");
      bytes = new Uint8Array([...bytes.slice(0, start), ...replacement, ...bytes.slice(end)]);
    }
    expect(new TextDecoder().decode(bytes)).toBe("😀 [alias] respondió. [alias] terminó.");
  });
  it("fails when a requested fragment is absent", () => {
    expect(() => findManualSpans("An answer", "Missing name")).toThrow("not found");
  });
});
