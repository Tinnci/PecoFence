// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", async (original) => ({
  ...(await original<typeof import("../src/api")>()),
  focus: mocks.focus,
}));
import { SourceFields } from "../src/SourceFields";
import { queryClient } from "../src/focus-cache";
import type { WorkItem } from "../src/contracts/setup";

afterEach(() => {
  cleanup();
  queryClient.clear();
});
it("shows complete source fields, distinguishes missing values and refreshes independently", async () => {
  const user = userEvent.setup();
  const item = {
    id: "123",
    source: "meegle",
    scope_id: "scope",
    url: "https://project.example/space/type/detail/123",
    updated: "revision",
  } as WorkItem;
  mocks.focus.mockImplementation(async (action: string) => ({
    source_fields: {
      item: item.id,
      address: item.url,
      item_updated: item.updated,
      fetched_at: new Date().toISOString(),
      fields: [
        {
          key: "cause",
          name: "原因分析",
          kind: "multi-text",
          available: true,
          value:
            action === "source_fields_refresh"
              ? "刷新后的分析"
              : "<p>分析首行</p><p>分析尾行</p>",
        },
        {
          key: "tags",
          name: "标签",
          kind: "multi-select",
          available: true,
          value: "OTA1",
        },
        {
          key: "test",
          name: "自测结果",
          kind: "text",
          available: true,
          value: "",
        },
        {
          key: "description",
          name: "描述",
          kind: "multi-text",
          available: false,
          value: "",
        },
      ],
    },
  }));
  render(<SourceFields code="MODEL" item={item} />);
  expect(await screen.findByText(/分析尾行/)).toBeTruthy();
  expect(screen.getByText("OTA1")).toBeTruthy();
  await user.click(
    screen.getByRole("button", { name: /显示空值及未返回字段/ }),
  );
  expect(screen.getByText("未填写")).toBeTruthy();
  expect(screen.getByText("来源未返回此字段")).toBeTruthy();
  await user.click(screen.getByRole("button", { name: "刷新" }));
  expect(await screen.findByText("刷新后的分析")).toBeTruthy();
  expect(mocks.focus).toHaveBeenCalledWith("source_fields_refresh", {
    code: "MODEL",
    item: "123",
    addresses: [item.url],
  });
});
