// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ loggingSettings: vi.fn() }));
vi.mock("../src/api", () => mocks);
import { SettingsDialog } from "../src/SettingsDialog";
import { LogSettings } from "../src/LogSettings";
import { queryClient } from "../src/focus-cache";
import type { LoggingSettings } from "../src/contracts/setup";
const policy = {
  schema_version: 1,
  retention_days: 14,
  max_total_mib: 200,
  daily_info_mib: 20,
  daily_max_mib: 40,
  daemon_file_mib: 20,
  daemon_files_per_stream: 14,
};
const settings: LoggingSettings = {
  policy,
  defaults: policy,
  settings_path: "C:\\Tinnci\\logging.json",
  directory: "C:\\Tinnci\\logs",
  daemon_directory: "C:\\Tinnci\\SPM\\logs",
  usage: { files: 9, bytes: 3 * 1048576 },
  daemon_usage: { files: 28, bytes: 1048576 },
};
beforeEach(() => {
  queryClient.clear();
  mocks.loggingSettings.mockReset();
  mocks.loggingSettings.mockResolvedValue(settings);
});
afterEach(cleanup);

it("opens logging settings from the existing global settings menu and saves through the owner CLI", async () => {
  const user = userEvent.setup();
  mocks.loggingSettings.mockImplementation(async (action, next) =>
    action === "set" ? { ...settings, policy: next } : settings,
  );
  render(
    <SettingsDialog
      open
      version="0.10.0"
      theme="light"
      onOpenChange={vi.fn()}
      onTheme={vi.fn()}
      onBackup={vi.fn()}
      onNewProject={vi.fn()}
    />,
  );
  expect(mocks.loggingSettings).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "日志与诊断" }));
  await screen.findByLabelText("保留天数");
  expect(screen.getByText("3.0 MiB · 9 份")).toBeTruthy();
  expect(screen.getByText(settings.directory)).toBeTruthy();
  await user.clear(screen.getByLabelText("保留天数"));
  await user.type(screen.getByLabelText("保留天数"), "30");
  await user.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() =>
    expect(mocks.loggingSettings).toHaveBeenCalledWith("set", {
      ...policy,
      retention_days: 30,
    }),
  );
  await screen.findByText("日志设置已保存。");
  expect((screen.getByLabelText("保留天数") as HTMLInputElement).value).toBe(
    "30",
  );
  await user.click(screen.getByRole("button", { name: "返回设置" }));
  expect(screen.getByRole("button", { name: "日志与诊断" })).toBeTruthy();
});

it("validates bounds and the relation between the daily limit and storage budget", async () => {
  const user = userEvent.setup();
  render(<LogSettings onBack={vi.fn()} />);
  await screen.findByLabelText("保留天数");
  await user.clear(screen.getByLabelText("保留天数"));
  await user.type(screen.getByLabelText("保留天数"), "0");
  expect(
    screen.getByRole("button", { name: "保存" }).hasAttribute("disabled"),
  ).toBe(true);
  expect(screen.getByRole("alert").textContent).toContain("请输入范围内的整数");
  await user.clear(screen.getByLabelText("保留天数"));
  await user.type(screen.getByLabelText("保留天数"), "30");
  await user.clear(screen.getByLabelText("共享日志存储预算（MiB）"));
  await user.type(screen.getByLabelText("共享日志存储预算（MiB）"), "10");
  expect(screen.getByRole("alert").textContent).toContain(
    "每日总上限不能超过共享存储预算",
  );
  expect(mocks.loggingSettings).toHaveBeenCalledTimes(1);
  await user.click(screen.getByRole("button", { name: "高级" }));
  expect(screen.getByLabelText("每份守护进程日志上限（MiB）")).toBeTruthy();
});

it("keeps edited values when saving fails and allows another attempt", async () => {
  const user = userEvent.setup();
  render(<LogSettings onBack={vi.fn()} />);
  await screen.findByLabelText("保留天数");
  mocks.loggingSettings.mockRejectedValue({
    code: "log_settings_io",
    message: "Access denied",
  });
  await user.clear(screen.getByLabelText("保留天数"));
  await user.type(screen.getByLabelText("保留天数"), "30");
  await user.click(screen.getByRole("button", { name: "保存" }));
  await screen.findByText(/Access denied/);
  expect((screen.getByLabelText("保留天数") as HTMLInputElement).value).toBe(
    "30",
  );
  expect(screen.queryByText("日志设置已保存。")).toBeNull();
  expect(
    screen.getByRole("button", { name: "保存" }).hasAttribute("disabled"),
  ).toBe(false);
});

it("can restore defaults even when the saved policy is unreadable", async () => {
  const user = userEvent.setup();
  mocks.loggingSettings
    .mockRejectedValueOnce({ code: "log_settings_io" })
    .mockResolvedValue(settings);
  render(<LogSettings onBack={vi.fn()} />);
  await screen.findByText("无法读取或保存日志设置。");
  await user.click(screen.getByRole("button", { name: "恢复默认" }));
  await waitFor(() =>
    expect(mocks.loggingSettings).toHaveBeenCalledWith("reset", undefined),
  );
  await screen.findByLabelText("保留天数");
  expect(screen.getByText("已恢复默认日志设置。")).toBeTruthy();
});
