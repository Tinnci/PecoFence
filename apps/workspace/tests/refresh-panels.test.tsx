// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", () => mocks);
import { CalendarPage } from "../src/Calendar";
import { MyIssues } from "../src/MyIssues";
import { queryClient } from "../src/focus-cache";
import { sourceHealth } from "../src/RefreshStatus";

beforeEach(() => {
  queryClient.clear();
  mocks.focus.mockReset();
});
afterEach(cleanup);

it("does not mark unknown source timestamps as refreshed", () => {
  expect(sourceHealth().outcome).toBe("unknown");
  expect(sourceHealth([{ label: "old source" }]).outcome).toBe("unknown");
  expect(
    sourceHealth([{ label: "bad date", updatedAt: "invalid" }]).outcome,
  ).toBe("unknown");
});

it("Calendar surfaces request failure and keeps its cached events", async () => {
  mocks.focus.mockImplementation(async (action) => {
    if (action === "calendar_cached")
      return {
        calendar: {
          computed_at: "2026-11-06T06:00:00Z",
          configured: true,
          sources: [{ source: "sheet", error: "", count: 0 }],
          events: [],
        },
      };
    if (action === "ota") return { ota: { models: [], errors: [] } };
    throw { code: "spm_timeout" };
  });
  render(<CalendarPage onModel={vi.fn()} />);
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toContain(
      "读取失败，沿用上次结果",
    ),
  );
  expect(screen.getByRole("alert").textContent).toContain("未及时响应");
  expect(screen.getByRole("status").textContent).toContain("上次更新");
});

it("My Issues does not hide a failed initial read behind an empty list", async () => {
  mocks.focus.mockRejectedValue({ code: "spm_timeout" });
  render(<MyIssues onModel={vi.fn()} />);
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toContain("未能读取来源"),
  );
  expect(screen.getByRole("alert").textContent).toContain("未及时响应");
  expect(screen.getByRole("button", { name: "重新读取" })).toBeTruthy();
});

it("Calendar identifies partial success from source health", async () => {
  mocks.focus.mockImplementation(async (action) =>
    action === "ota"
      ? { ota: { models: [], errors: [] } }
      : {
          calendar: {
            computed_at: "2026-11-06T06:00:00Z",
            configured: true,
            sources: [
              { source: "sheet", error: "", count: 0 },
              { source: "base", error: "Board unavailable", count: 0 },
            ],
            events: [],
          },
        },
  );
  render(<CalendarPage onModel={vi.fn()} />);
  await waitFor(() =>
    expect(screen.getByRole("status").textContent).toContain("部分更新"),
  );
  expect(screen.getByRole("alert").textContent).toContain("Board unavailable");
});
