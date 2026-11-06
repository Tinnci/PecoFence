// @vitest-environment jsdom
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", () => mocks);
import {
  Platforms,
  ModelPlatform,
  platformsKey,
  profilesKey,
  profileWords,
} from "../src/Platforms";
import { queryClient } from "../src/focus-cache";
import type { ModelProfile, Platform } from "../src/contracts/setup";
const platform: Platform = {
  code: "P6700LV",
  vendor: "Unisoc",
  chip: "UMS9621S",
  baseband: "",
  modem: ["4G_MODEM_22B_W26.02.5_P2"],
  modem_repos: ["modem-branch"],
  modem_changes: 3,
  android: ["A16"],
  kernel: ["5.15"],
  evidence: [],
};
beforeEach(() => {
  queryClient.clear();
  mocks.focus.mockReset();
  mocks.focus.mockResolvedValue({ platforms: [platform] });
});
afterEach(() => {
  cleanup();
  queryClient.clear();
});
it("saves only the edited baseband and refreshes the shared model header", async () => {
  queryClient.setQueryData(platformsKey, [platform]);
  mocks.focus.mockImplementation(async (action, args) => {
    if (action === "chip") return {};
    return { platforms: [{ ...platform, baseband: "MOLY.LR12A" }] };
  });
  render(
    <>
      <Platforms models={null} />
      <ModelPlatform code="P6700LV" />
    </>,
  );
  const field = screen.getByRole("textbox", { name: "P6700LV 的基带版本" });
  fireEvent.change(field, { target: { value: " MOLY.LR12A " } });
  fireEvent.blur(field);
  await waitFor(() =>
    expect(mocks.focus).toHaveBeenCalledWith("chip", {
      code: "P6700LV",
      baseband: "MOLY.LR12A",
    }),
  );
  await screen.findByText(/基带 MOLY.LR12A/);
  expect(screen.getByText(/4G_MODEM_22B/)).toBeTruthy();
  expect(screen.getByText(/modem-branch/)).toBeTruthy();
});
it("fills an empty baseband from the newest modem package", async () => {
  queryClient.setQueryData(platformsKey, [platform]);
  mocks.focus.mockResolvedValue({ platforms: [platform] });
  render(<Platforms models={null} />);
  fireEvent.click(
    screen.getByRole("button", { name: "4G_MODEM_22B_W26.02.5_P2" }),
  );
  await waitFor(() =>
    expect(mocks.focus).toHaveBeenCalledWith("chip", {
      code: "P6700LV",
      baseband: "4G_MODEM_22B_W26.02.5_P2",
    }),
  );
});
it("keeps a failed edit available for retry and supports clearing a value", async () => {
  queryClient.setQueryData(platformsKey, [{ ...platform, baseband: "OLD" }]);
  mocks.focus.mockRejectedValue(new Error("offline"));
  render(<Platforms models={null} />);
  const field = screen.getByRole("textbox", { name: "P6700LV 的基带版本" });
  fireEvent.change(field, { target: { value: "" } });
  fireEvent.blur(field);
  await waitFor(() => expect(field.getAttribute("aria-invalid")).toBe("true"));
  expect(mocks.focus).toHaveBeenCalledWith("chip", {
    code: "P6700LV",
    baseband: "",
  });
  expect((field as HTMLInputElement).value).toBe("");
});

it("edits from the model header and preserves untouched fields after a refresh", async () => {
  queryClient.setQueryData(platformsKey, [platform]);
  mocks.focus.mockImplementation(async (action) =>
    action === "chip"
      ? {}
      : {
          platforms: [
            { ...platform, chip: "NEW CHIP", baseband: "NEW BASEBAND" },
          ],
        },
  );
  const user = userEvent.setup();
  render(<ModelPlatform code="P6700LV" />);
  await user.click(screen.getByRole("button", { name: "P6700LV 的平台信息" }));
  // A concurrent refresh must not make the untouched input overwrite newer data.
  queryClient.setQueryData(platformsKey, [{ ...platform, chip: "NEW CHIP" }]);
  await user.type(
    screen.getByRole("textbox", { name: "基带版本" }),
    "NEW BASEBAND",
  );
  await user.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() =>
    expect(mocks.focus).toHaveBeenCalledWith("chip", {
      code: "P6700LV",
      baseband: "NEW BASEBAND",
    }),
  );
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});
it("cancels header edits without persisting them", async () => {
  queryClient.setQueryData(platformsKey, [platform]);
  const user = userEvent.setup();
  render(<ModelPlatform code="P6700LV" />);
  await user.click(screen.getByRole("button", { name: "P6700LV 的平台信息" }));
  await user.type(screen.getByRole("textbox", { name: "基带版本" }), "UNSAVED");
  await user.click(screen.getByRole("button", { name: "取消" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(mocks.focus.mock.calls.some(([action]) => action === "chip")).toBe(
    false,
  );
});
it("reloads platform facts explicitly from the header", async () => {
  queryClient.setQueryData(platformsKey, [platform]);
  mocks.focus.mockResolvedValue({
    platforms: [{ ...platform, chip: "UPDATED CHIP" }],
  });
  render(<ModelPlatform code="P6700LV" />);
  fireEvent.click(screen.getByRole("button", { name: "重新读取平台信息" }));
  await screen.findByText(/UPDATED CHIP/);
  expect(mocks.focus).toHaveBeenCalledWith("platforms");
});

const origin = (document: string, place: string, url = "") => ({
  role: "chip",
  document,
  place,
  field: "平台",
  value: place,
  url,
});
const open: ModelProfile = {
  code: "P6700LV",
  aliases: ["P6700LV-A"],
  suggested_aliases: ["P6700LV-B"],
  platform: { ...platform, chip: "" },
  read_at: "",
  phase: "",
  fields: [
    {
      field: "chip",
      value: "",
      conflict: true,
      values: [
        {
          value: "Unisoc T606",
          origins: [origin("sheet", "B9", "https://feishu.example/B9")],
        },
        { value: "Unisoc T765", origins: [origin("base", "P6700LV V15")] },
      ],
    },
  ],
};
it("shows disagreeing documents and saves the chip you pick", async () => {
  queryClient.setQueryData(platformsKey, [{ ...platform, chip: "" }]);
  queryClient.setQueryData(profilesKey, [open]);
  mocks.focus.mockResolvedValue({ platforms: [platform], profiles: [open] });
  const user = userEvent.setup();
  render(<ModelPlatform code="P6700LV" />);
  const line = screen.getByRole("button", { name: "P6700LV 的平台信息" });
  expect(line.textContent).toContain("Unisoc T606 / Unisoc T765");
  await user.click(line);
  expect(
    screen.getByRole("link", { name: "Unisoc T606" }).getAttribute("href"),
  ).toBe("https://feishu.example/B9");
  await user.click(screen.getByRole("button", { name: "用 Unisoc T765" }));
  await waitFor(() =>
    expect(mocks.focus).toHaveBeenCalledWith("chip", {
      code: "P6700LV",
      chip: "Unisoc T765",
    }),
  );
});
it("ties a near name in the documents to the model", async () => {
  queryClient.setQueryData(platformsKey, [platform]);
  queryClient.setQueryData(profilesKey, [open]);
  mocks.focus.mockResolvedValue({ platforms: [platform], profiles: [open] });
  const user = userEvent.setup();
  render(<ModelPlatform code="P6700LV" />);
  await user.click(screen.getByRole("button", { name: "P6700LV 的平台信息" }));
  await user.click(screen.getByRole("button", { name: "同一机型：P6700LV-B" }));
  await waitFor(() =>
    expect(mocks.focus).toHaveBeenCalledWith("alias", {
      code: "P6700LV",
      alias: "P6700LV-B",
    }),
  );
  await user.click(screen.getByRole("button", { name: "移除 P6700LV-A" }));
  await waitFor(() =>
    expect(mocks.focus).toHaveBeenCalledWith("alias_remove", {
      code: "P6700LV",
      alias: "P6700LV-A",
    }),
  );
});
it("finds a model by what the documents say about it", () => {
  expect(profileWords(open)).toContain("Unisoc T765");
  expect(profileWords(open)).toContain("P6700LV-A");
});
