import { describe, expect, it, vi } from "vitest";
import { matchCommands, moveActive, type Command } from "../src/palette";

const run = vi.fn();
const commands: Command[] = [
  { id: "a", label: "Activity", run },
  { id: "t", label: "链路", words: "traces waterfall", run },
  { id: "m", label: "My Model", words: "E6765GDL", typed: true, run },
  { id: "k", label: "Tracking", typed: true, run },
];

describe("command palette", () => {
  it("keeps typed-only commands out of an empty palette", () => {
    expect(matchCommands(commands, " ").map((c) => c.id)).toEqual(["a", "t"]);
  });
  it("finds by other words and ranks names that start with the text first", () => {
    expect(matchCommands(commands, "e6765").map((c) => c.id)).toEqual(["m"]);
    expect(matchCommands(commands, "tra").map((c) => c.id)).toEqual(["k", "t"]);
  });
  it("wraps at both ends", () => {
    expect(moveActive(2, 1, 3)).toBe(0);
    expect(moveActive(0, -1, 3)).toBe(2);
    expect(moveActive(0, 1, 0)).toBe(0);
  });
});
