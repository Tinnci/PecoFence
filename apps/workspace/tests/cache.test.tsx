// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", () => mocks);
import { useBuilds } from "../src/Model";
import { useFocus } from "../src/Focus";
import {
  buildsOptions,
  invalidateModel,
  modelsKey,
  prefetchBuilds,
  queryClient,
  refreshModel,
  summaryKey,
} from "../src/focus-cache";
import type { FocusModel, ModelSummary } from "../src/contracts/setup";
import { useModelPreference } from "../src/model-navigation";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
const builds = (code: string) => ({
  code,
  computed_at: "2020-01-01T00:00:00Z",
  jobs: [],
  errors: [],
});
const model = (code: string, issues = 0) =>
  ({
    code,
    issues,
    flagged: [],
    targets: { severe_pct: 95, overall_pct: 85, fatal_max: 0 },
  }) as unknown as ModelSummary;
function BuildView({ code }: { code: string }) {
  const state = useBuilds(code);
  return (
    <>
      <output>{state.data?.code ?? "loading"}</output>
      <span>{state.failure?.code}</span>
      <button onClick={() => void state.refresh()}>refresh</button>
    </>
  );
}
function SummaryView() {
  const state = useFocus(true);
  return (
    <>
      <output>
        {state.summary?.models
          .map((item) => `${item.code}:${item.issues}`)
          .join(",")}
      </output>
      <span>{state.failure?.code}</span>
    </>
  );
}
beforeEach(() => {
  queryClient.clear();
  mocks.focus.mockReset();
});
afterEach(async () => {
  cleanup();
  await queryClient.cancelQueries();
  queryClient.clear();
});

describe("model cache", () => {
  it("isolates late build responses and restores cached data immediately when returning", async () => {
    const a = deferred<{ builds: ReturnType<typeof builds> }>();
    const b = deferred<{ builds: ReturnType<typeof builds> }>();
    mocks.focus.mockImplementation((action, args) =>
      action === "builds_cached"
        ? Promise.resolve({})
        : args.code === "A"
          ? a.promise
          : b.promise,
    );
    const view = render(<BuildView code="A" />);
    await waitFor(() =>
      expect(mocks.focus).toHaveBeenCalledWith("builds", { code: "A" }),
    );
    view.rerender(<BuildView code="B" />);
    await waitFor(() =>
      expect(mocks.focus).toHaveBeenCalledWith("builds", { code: "B" }),
    );
    await act(async () => b.resolve({ builds: builds("B") }));
    await screen.findByText("B", { selector: "output" });
    await act(async () => a.resolve({ builds: builds("A") }));
    expect(screen.getByRole("status").textContent).toBe("B");
    view.rerender(<BuildView code="A" />);
    expect(screen.getByRole("status").textContent).toBe("A");
    expect(
      mocks.focus.mock.calls.filter((call) => call[0] === "builds"),
    ).toHaveLength(2);
  });

  it("deduplicates prefetches and keeps data when a refresh fails", async () => {
    const fresh = deferred<{ builds: ReturnType<typeof builds> }>();
    mocks.focus.mockImplementation((action) =>
      action === "builds_cached"
        ? Promise.resolve({ builds: builds("A") })
        : fresh.promise,
    );
    const first = prefetchBuilds("A"),
      second = prefetchBuilds("A");
    await waitFor(() =>
      expect(
        mocks.focus.mock.calls.filter((call) => call[0] === "builds"),
      ).toHaveLength(1),
    );
    expect(queryClient.getQueryData(buildsOptions("A").queryKey)).toMatchObject(
      { code: "A" },
    );
    fresh.reject({ code: "spm_timeout" });
    await Promise.all([first, second]);
    render(<BuildView code="A" />);
    expect(screen.getByRole("status").textContent).toBe("A");
    await screen.findByText("spm_timeout");
  });

  it("publishes a model summary before the next model finishes and keeps refreshing after a failure", async () => {
    const b = deferred<unknown>();
    mocks.focus.mockImplementation(async (action, args) => {
      if (action === "list")
        return { models: [{ code: "A" }, { code: "B" }, { code: "C" }] };
      if (action === "cached")
        return { summary: { models: [model("A"), model("B"), model("C")] } };
      if (args.code === "B") return b.promise;
      return { summary: { models: [model(args.code, 10)] } };
    });
    render(<SummaryView />);
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain("A:10"),
    );
    expect(screen.getByRole("status").textContent).toContain("B:0");
    b.reject({ code: "spm_timeout" });
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain("C:10"),
    );
    await screen.findByText("spm_timeout");
  });

  it("discards a summary that finishes after a model was changed", async () => {
    const old = deferred<unknown>();
    queryClient.setQueryData(modelsKey, [{ code: "A" }] as FocusModel[]);
    queryClient.setQueryData(summaryKey, { models: [model("A", 3)] });
    mocks.focus.mockReturnValue(old.promise);
    const read = refreshModel("A");
    await waitFor(() => expect(mocks.focus).toHaveBeenCalled());
    invalidateModel("A");
    old.resolve({ summary: { models: [model("A", 999)] } });
    await read;
    expect(queryClient.getQueryData(summaryKey)).toMatchObject({
      models: [{ issues: 3 }],
    });
  });

  it("restores each model's tab independently", async () => {
    function View({ code }: { code: string }) {
      const [tab, setTab] = useModelPreference(code, "tab", "overview");
      return (
        <button onClick={() => setTab("branches")}>
          {code}:{tab}
        </button>
      );
    }
    const view = render(<View code="A" />);
    await act(async () => screen.getByRole("button").click());
    await screen.findByText("A:branches");
    view.rerender(<View code="B" />);
    expect(screen.getByRole("button").textContent).toBe("B:overview");
    view.rerender(<View code="A" />);
    expect(screen.getByRole("button").textContent).toBe("A:branches");
  });
  it("refreshes from sources when the startup disk cache cannot be read", async () => {
    mocks.focus.mockImplementation(async (action) => {
      if (action === "list") return { models: [{ code: "A" }] };
      if (action === "cached") throw { code: "spm_invalid_response" };
      return { summary: { models: [model("A", 12)] } };
    });
    render(<SummaryView />);
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toBe("A:12"),
    );
    expect(screen.queryByText("spm_invalid_response")).toBeNull();
  });

  it("rolls back a failed review without undoing another successful review", async () => {
    const failed = deferred<unknown>();
    const original = {
      ...model("A"),
      flagged: [
        { number: 1, tracked: false },
        { number: 2, tracked: false },
      ],
    };
    mocks.focus.mockImplementation(async (action, args) => {
      if (action === "list") return { models: [{ code: "A" }] };
      if (action === "review") return args.change === 1 ? failed.promise : {};
      return { summary: { models: [original] } };
    });
    function View() {
      const state = useFocus(true);
      return (
        <>
          <output>
            {state.summary?.models[0]?.flagged
              .map((item) => item.number)
              .join(",")}
          </output>
          <button onClick={() => void state.review("A", 1, "accept")}>
            first
          </button>
          <button onClick={() => void state.review("A", 2, "accept")}>
            second
          </button>
        </>
      );
    }
    render(<View />);
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toBe("1,2"),
    );
    await waitFor(() =>
      expect(mocks.focus).toHaveBeenCalledWith("refresh", { code: "A" }),
    );
    await act(async () => {
      screen.getByText("first").click();
    });
    await act(async () => {
      screen.getByText("second").click();
    });
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toBe(""),
    );
    await act(async () => failed.reject({ code: "source_failed" }));
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toBe("1"),
    );
  });
  it("hides models left in the disk cache after they were unfollowed", async () => {
    mocks.focus.mockImplementation(async (action) =>
      action === "list"
        ? { models: [] }
        : { summary: { models: [model("removed", 12)] } },
    );
    render(<SummaryView />);
    await waitFor(() => expect(mocks.focus).toHaveBeenCalledWith("list"));
    await waitFor(() => expect(mocks.focus).toHaveBeenCalledWith("cached"));
    expect(screen.getByRole("status").textContent).toBe("");
    expect(mocks.focus.mock.calls.some((call) => call[0] === "refresh")).toBe(
      false,
    );
  });
});
