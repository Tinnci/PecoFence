import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("index.html", () => {
  it("has no <style> block, which would disable inline styles for the whole app", () => {
    // Tauri nonces a <style> tag into the CSP, and a nonce makes 'unsafe-inline' ignored, which
    // blocks the styles NumberFlow puts in its shadow DOM.
    const html = readFileSync("index.html", "utf8");
    expect(html).not.toMatch(/<style/i);
  });
});
