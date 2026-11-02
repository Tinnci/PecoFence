// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({
  openUrl: vi.fn(),
  revealItemInDir: vi.fn(),
  writeText: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: mocks.openUrl,
  revealItemInDir: mocks.revealItemInDir,
}));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: mocks.writeText,
}));
import {
  ContextMenuProvider,
  ContextTarget,
} from "../src/components/context-menu";
import { ExternalLink } from "../src/components/external-link";
import {
  commitContextActions,
  buildContextActions,
  revealContextAction,
} from "../src/context-actions";
import ConnectSources from "../src/ConnectSources";
import { setLanguage } from "../src/i18n";

afterEach(() => {
  cleanup();
  window.getSelection()?.removeAllRanges();
  vi.resetAllMocks();
});
function open(target: Element) {
  fireEvent.contextMenu(target, { button: 2, clientX: 120, clientY: 80 });
}

describe("shared context menu", () => {
  it("opens the target's actions without activating its left-click behavior", async () => {
    const leftClick = vi.fn(),
      choose = vi.fn();
    render(
      <ContextMenuProvider>
        <ContextTarget
          title="Model A"
          actions={[{ id: "one", label: "View builds", run: choose }]}
        >
          <button onClick={leftClick}>A</button>
        </ContextTarget>
        <ContextTarget
          title="Model B"
          actions={[{ id: "two", label: "Copy B", run: choose }]}
        >
          <button>B</button>
        </ContextTarget>
      </ContextMenuProvider>,
    );
    open(screen.getByRole("button", { name: "A" }));
    expect(
      await screen.findByRole("menuitem", { name: "View builds" }),
    ).toBeTruthy();
    expect(screen.queryByRole("menuitem", { name: "Copy B" })).toBeNull();
    expect(leftClick).not.toHaveBeenCalled();
    await userEvent.click(
      screen.getByRole("menuitem", { name: "View builds" }),
    );
    expect(choose).toHaveBeenCalledOnce();
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
  });

  it("supports Shift+F10, arrow navigation, Escape and returning focus to the target", async () => {
    const first = vi.fn(),
      second = vi.fn();
    const user = userEvent.setup();
    render(
      <ContextMenuProvider>
        <ContextTarget
          actions={[
            { id: "first", label: "First", run: first },
            { id: "second", label: "Second", run: second },
          ]}
        >
          <button>Model</button>
        </ContextTarget>
      </ContextMenuProvider>,
    );
    const target = screen.getByRole("button", { name: "Model" });
    await user.tab();
    await user.keyboard("{Shift>}{F10}{/Shift}");
    await screen.findByRole("menu");
    await user.keyboard("{Home}{ArrowDown}{Enter}");
    await waitFor(() => expect(second).toHaveBeenCalledOnce());
    expect(first).not.toHaveBeenCalled();
    await waitFor(() => expect(document.activeElement).toBe(target));
    await user.keyboard("{Shift>}{F10}{/Shift}");
    await screen.findByRole("menu");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(document.activeElement).toBe(target);
  });

  it("preserves native menus for editing and unregistered text", () => {
    render(
      <ContextMenuProvider>
        <ContextTarget actions={[{ id: "copy", label: "Copy", run: vi.fn() }]}>
          <div>
            <input aria-label="Name" />
            <span>Row</span>
          </div>
        </ContextTarget>
        <p>Plain text</p>
      </ContextMenuProvider>,
    );
    for (const target of [
      screen.getByRole("textbox"),
      screen.getByText("Plain text"),
    ]) {
      const event = new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        button: 2,
      });
      fireEvent(target, event);
      expect(event.defaultPrevented).toBe(false);
    }
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("uses the system opener and clipboard, keeps failures retryable and blocks duplicate actions", async () => {
    const user = userEvent.setup();
    mocks.writeText
      .mockRejectedValueOnce(new Error("unavailable"))
      .mockResolvedValueOnce(undefined);
    render(
      <ContextMenuProvider>
        <ExternalLink href="https://review.example.test/c/123">
          Commit
        </ExternalLink>
      </ContextMenuProvider>,
    );
    open(screen.getByRole("link"));
    await user.click(
      await screen.findByRole("menuitem", { name: "复制链接地址" }),
    );
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByRole("menu")).toBeTruthy();
    await user.click(screen.getByRole("menuitem", { name: "复制链接地址" }));
    expect(mocks.writeText).toHaveBeenLastCalledWith(
      "https://review.example.test/c/123",
    );
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    open(screen.getByRole("link"));
    await user.click(
      await screen.findByRole("menuitem", { name: "在浏览器中打开" }),
    );
    expect(mocks.openUrl).toHaveBeenCalledExactlyOnceWith(
      "https://review.example.test/c/123",
    );
  });

  it("gives commits title/number copy and review actions even without a URL", async () => {
    const review = vi.fn();
    render(
      <ContextMenuProvider>
        <ContextTarget
          title="Gerrit #123"
          actions={commitContextActions(
            { number: 123, subject: "Fix Wi-Fi", tracked: true },
            review,
          )}
        >
          <article>Fix Wi-Fi</article>
        </ContextTarget>
      </ContextMenuProvider>,
    );
    open(screen.getByRole("article"));
    expect(
      await screen.findByRole("menuitem", { name: "复制提交标题" }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("menuitem", { name: "在浏览器中打开" }),
    ).toBeNull();
    await userEvent.click(screen.getByRole("menuitem", { name: "停止跟踪" }));
    expect(review).toHaveBeenCalledWith("untrack");
  });

  it("translates actions on the next open without changing the underlying target", async () => {
    render(
      <ContextMenuProvider>
        <ExternalLink href="https://review.example.test/c/123">
          Commit
        </ExternalLink>
      </ContextMenuProvider>,
    );
    await act(async () => {
      await setLanguage("en");
    });
    open(screen.getByRole("link"));
    expect(
      await screen.findByRole("menuitem", { name: "Copy link address" }),
    ).toBeTruthy();
    expect(
      screen.getByRole("link", { hidden: true }).getAttribute("href"),
    ).toBe("https://review.example.test/c/123");
  });
  it("copies only the selected text when the selection belongs to the target", async () => {
    render(
      <ContextMenuProvider>
        <ContextTarget actions={[{ id: "noop", label: "Other", run: vi.fn() }]}>
          <article>Fix WiFi connectivity</article>
        </ContextTarget>
      </ContextMenuProvider>,
    );
    const article = screen.getByRole("article");
    const range = document.createRange();
    range.setStart(article.firstChild!, 4);
    range.setEnd(article.firstChild!, 8);
    window.getSelection()?.addRange(range);
    open(article);
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "复制选中文本" }),
    );
    expect(mocks.writeText).toHaveBeenCalledExactlyOnceWith("WiFi");
  });

  it("disables duplicate actions and ignores an old action's failure after opening another target", async () => {
    let reject!: (error: Error) => void;
    const pending = new Promise<void>((_, fail) => {
      reject = fail;
    });
    const first = vi.fn(() => pending),
      second = vi.fn();
    const user = userEvent.setup();
    render(
      <ContextMenuProvider>
        <ContextTarget actions={[{ id: "first", label: "First", run: first }]}>
          <button>A</button>
        </ContextTarget>
        <ContextTarget
          actions={[{ id: "second", label: "Second", run: second }]}
        >
          <button>B</button>
        </ContextTarget>
      </ContextMenuProvider>,
    );
    open(screen.getByRole("button", { name: "A" }));
    await user.click(await screen.findByRole("menuitem", { name: "First" }));
    expect(
      screen
        .getByRole("menuitem", { name: "First" })
        .getAttribute("aria-disabled"),
    ).toBe("true");
    fireEvent.click(screen.getByRole("menuitem", { name: "First" }));
    expect(first).toHaveBeenCalledOnce();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    open(screen.getByRole("button", { name: "B" }));
    await screen.findByRole("menuitem", { name: "Second" });
    await act(async () => reject(new Error("late failure")));
    expect(screen.queryByRole("alert")).toBeNull();
    await user.click(screen.getByRole("menuitem", { name: "Second" }));
    expect(second).toHaveBeenCalledOnce();
  });
});

it("opens the selected historical build console and copies its number", async () => {
  const user = userEvent.setup();
  render(
    <ContextMenuProvider>
      <ContextTarget
        actions={buildContextActions({
          number: 42,
          url: "http://ci.test/jenkins/job/mobile/42/",
          building: false,
          timestamp: 0,
          duration: 100,
          estimated: 100,
        })}
      >
        <span>Build 42</span>
      </ContextTarget>
    </ContextMenuProvider>,
  );
  open(screen.getByText("Build 42"));
  await user.click(await screen.findByRole("menuitem", { name: "构建日志" }));
  expect(mocks.openUrl).toHaveBeenCalledWith(
    "http://ci.test/jenkins/job/mobile/42/console",
  );
  open(screen.getByText("Build 42"));
  await user.click(
    await screen.findByRole("menuitem", { name: "复制构建编号" }),
  );
  expect(mocks.writeText).toHaveBeenCalledWith("42");
});
it("keeps branch searches scoped to the commit repository", async () => {
  const user = userEvent.setup();
  render(
    <ContextMenuProvider>
      <ContextTarget
        actions={commitContextActions({
          number: 12,
          subject: "Fix",
          url: "http://review.test/review/c/mobile/+/12",
          project: "mobile/apps",
          branch: "release",
          owner: "Dev",
        })}
      >
        <span>Change 12</span>
      </ContextTarget>
    </ContextMenuProvider>,
  );
  open(screen.getByText("Change 12"));
  await user.click(
    await screen.findByRole("menuitem", { name: "查看分支的提交" }),
  );
  const url = mocks.openUrl.mock.calls[0][0];
  expect(url.startsWith("http://review.test/review/q/")).toBe(true);
  expect(decodeURIComponent(url.split("/q/")[1])).toBe(
    'project:"mobile/apps" branch:"release"',
  );
});

it("opens a connected server, copies its address and rechecks from its menu", async () => {
  const user = userEvent.setup(),
    recheck = vi.fn().mockResolvedValue(undefined);
  render(
    <ContextMenuProvider>
      <ConnectSources
        report={{
          checked_at: "2026-11-02T00:00:00Z",
          connections: [
            {
              source: "jenkins",
              tool: "jenkins-bridge",
              state: "connected",
              host: "http://ci.test/jenkins/",
              sign_in: false,
            },
          ],
        }}
        failure={null}
        onRefresh={recheck}
        onReport={() => {}}
        onOpen={() => {}}
      />
    </ContextMenuProvider>,
  );
  const row = screen.getByRole("listitem", { name: "Jenkins" });
  open(row);
  await user.click(
    await screen.findByRole("menuitem", { name: "复制服务器地址" }),
  );
  expect(mocks.writeText).toHaveBeenCalledWith("http://ci.test/jenkins/");
  open(row);
  await user.click(await screen.findByRole("menuitem", { name: "重新检查" }));
  expect(recheck).toHaveBeenCalledOnce();
  await user.click(screen.getByRole("link", { name: "Jenkins" }));
  expect(mocks.openUrl).toHaveBeenCalledWith("http://ci.test/jenkins/");
});
it("reveals an imported configuration folder through the native opener", async () => {
  const action = revealContextAction("C:\\Users\\User\\AppData\\Local\\SPM");
  expect(action).not.toBeNull();
  await action!.run();
  expect(mocks.revealItemInDir).toHaveBeenCalledWith(
    "C:\\Users\\User\\AppData\\Local\\SPM",
  );
});
