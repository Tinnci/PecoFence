// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => false }));
import { IssueBrowser, LinkedIssues, filterIssues } from "../src/IssueBrowser";
import type { IssueLink } from "../src/contracts/setup";
const issues: IssueLink[] = [
  {
    id: "APP-1",
    url: "http://jira.test/jira/browse/APP-1",
    resolved: false,
    severe: true,
    fatal: true,
  },
  {
    id: "APP-2",
    url: "http://jira.test/jira/browse/APP-2",
    resolved: true,
    severe: true,
    fatal: false,
  },
  {
    id: "20793",
    url: "https://meegle.test/team/issue/detail/20793",
    resolved: false,
    severe: false,
    fatal: false,
  },
];
afterEach(cleanup);
it("filters with gate classifications and searches native issue IDs", () => {
  expect(filterIssues(issues, "all")).toHaveLength(3);
  expect(filterIssues(issues, "unresolved")).toHaveLength(2);
  expect(filterIssues(issues, "severe").map((i) => i.id)).toEqual(["APP-1"]);
  expect(filterIssues(issues, "fatal").map((i) => i.id)).toEqual(["APP-1"]);
  expect(filterIssues(issues, "all", " app-2 ").map((i) => i.id)).toEqual([
    "APP-2",
  ]);
});
it("opens the requested issue filter, switches filters and searches", async () => {
  const user = userEvent.setup();
  render(<IssueBrowser issues={issues} open="fatal" onClose={() => {}} />);
  expect(await screen.findByRole("link", { name: "APP-1" })).toBeTruthy();
  expect(screen.queryByRole("link", { name: "APP-2" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "全部问题" }));
  expect(screen.getByRole("link", { name: "APP-2" }).getAttribute("href")).toBe(
    issues[1].url,
  );
  await user.type(screen.getByRole("textbox"), "20793");
  expect(screen.queryByRole("link", { name: "APP-1" })).toBeNull();
  expect(screen.getByRole("link", { name: "20793" })).toBeTruthy();
});
it("links only an unambiguous source record, including numeric Bug references", () => {
  render(
    <LinkedIssues
      ids={["APP-1", "#20793", "Bug #20793", "UNKNOWN-1"]}
      issues={issues}
    />,
  );
  expect(
    screen.getByRole("link", { name: "Bug #20793" }).getAttribute("href"),
  ).toBe(issues[2].url);
  expect(screen.queryByRole("link", { name: "UNKNOWN-1" })).toBeNull();
  cleanup();
  render(
    <LinkedIssues
      ids={["APP-1"]}
      issues={[
        ...issues,
        { ...issues[0], url: "https://other.test/browse/APP-1" },
      ]}
    />,
  );
  expect(screen.queryByRole("link")).toBeNull();
});
