// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
const mocks = vi.hoisted(() => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/plugin-opener", () => mocks);
import { browserUrl, ExternalLink } from "../src/components/external-link";
import { consoleBrowserUrl, jobBrowserUrl } from "../src/browser-links";
import type { JobView } from "../src/contracts/setup";

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
describe("browser links", () => {
  it("opens commit URLs in the system browser and preserves a copyable hyperlink", async () => {
    const user = userEvent.setup();
    mocks.openUrl.mockResolvedValue(undefined);
    render(
      <ExternalLink href="https://review.example.test/c/mobile/+/80020">
        Fix connectivity
      </ExternalLink>,
    );
    const link = screen.getByRole("link", { name: "Fix connectivity" });
    expect(link.getAttribute("href")).toBe(
      "https://review.example.test/c/mobile/+/80020",
    );
    expect(link.getAttribute("rel")).toBe("noopener noreferrer");
    await user.tab();
    expect(document.activeElement).toBe(link);
    await user.keyboard("{Enter}");
    expect(mocks.openUrl).toHaveBeenCalledExactlyOnceWith(
      "https://review.example.test/c/mobile/+/80020",
    );
  });

  it("keeps missing, malformed and executable URLs as plain text", () => {
    for (const value of [
      undefined,
      "/relative",
      "javascript:alert(1)",
      "file:///etc/passwd",
      "data:text/html,test",
      "https://user:password@example.test/",
      "https://example.test/\n",
    ]) {
      expect(browserUrl(value)).toBeNull();
      const view = render(<ExternalLink href={value}>Commit</ExternalLink>);
      expect(screen.queryByRole("link")).toBeNull();
      view.unmount();
    }
    expect(browserUrl("http://review.internal:8080/c/123")).toBe(
      "http://review.internal:8080/c/123",
    );
  });

  it("shows browser-opening failures and allows retrying the same link", async () => {
    const user = userEvent.setup();
    mocks.openUrl
      .mockRejectedValueOnce(new Error("unavailable"))
      .mockResolvedValueOnce(undefined);
    render(
      <ExternalLink href="https://review.example.test/123">
        Commit
      </ExternalLink>,
    );
    await user.click(screen.getByRole("link"));
    expect(await screen.findByRole("alert")).toBeTruthy();
    await user.click(screen.getByRole("link"));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    expect(mocks.openUrl).toHaveBeenCalledTimes(2);
  });

  it("derives job and console links from real Jenkins build URLs, including context paths", () => {
    const run = {
      number: 96,
      url: "https://ci.example.test/jenkins/job/Mobile/job/Android%2016/96/",
      building: false,
      timestamp: 0,
      duration: 1,
      estimated: 1,
    };
    const job: JobView = {
      job: "Mobile/Android 16",
      name: "Android 16",
      phase: "main",
      disabled: false,
      parameters: {},
      changes: [],
      builds: [run],
    };
    expect(jobBrowserUrl(job)).toBe(
      "https://ci.example.test/jenkins/job/Mobile/job/Android%2016/",
    );
    expect(consoleBrowserUrl(run)).toBe(
      "https://ci.example.test/jenkins/job/Mobile/job/Android%2016/96/console",
    );
    expect(
      jobBrowserUrl({ ...job, builds: [{ ...run, url: undefined }] }),
    ).toBeNull();
    expect(
      consoleBrowserUrl({ ...run, url: "https://ci.example.test/login" }),
    ).toBeNull();
  });
});
