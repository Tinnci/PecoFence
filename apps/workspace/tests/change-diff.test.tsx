// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
const mocks = vi.hoisted(() => ({ focus: vi.fn() }));
vi.mock("../src/api", async (original) => ({
  ...(await original<typeof import("../src/api")>()),
  focus: mocks.focus,
}));
import {
  ChangeFiles,
  ChangeStatus,
  diffLines,
  parseMessage,
} from "../src/ChangeDiff";
import { queryClient } from "../src/focus-cache";

const diff = [
  "diff --git a/buildspec/P.spec b/buildspec/P.spec",
  "index c3555f0..5d20f2f 100755",
  "--- a/buildspec/P.spec",
  "+++ b/buildspec/P.spec",
  "@@ -35,4 +35,4 @@ CONFIG",
  " ; VERSION",
  "-SW = V14",
  "+SW = V15",
  " HW = V1.0",
  "",
].join("\n");

beforeEach(() => {
  queryClient.clear();
  mocks.focus.mockReset().mockImplementation(async (action: string) =>
    action === "change_files"
      ? {
          change_files: {
            change: 80481,
            message:
              "[HM][P][SPM][Feature]:改 spec\n\nST-result:[PASS]\n\nChange-Id: I1",
            files: [
              {
                path: "buildspec/P.spec",
                kind: "MODIFIED",
                insertions: 1,
                deletions: 1,
              },
            ],
          },
        }
      : {
          change_diff: {
            change: 80481,
            path: "buildspec/P.spec",
            patch_set: 1,
            diff,
            truncated: false,
          },
        },
  );
});

describe("change diffs", () => {
  it("numbers each line from its hunk", () => {
    const lines = diffLines(diff).filter((l) => l.kind !== "meta");
    expect(lines.map((l) => [l.kind, l.old, l.new])).toEqual([
      ["hunk", undefined, undefined],
      ["ctx", 35, 35],
      ["del", 36, undefined],
      ["add", undefined, 36],
      ["ctx", 37, 37],
    ]);
  });

  it("reads files only when opened, then one file's diff", async () => {
    const user = userEvent.setup();
    render(<ChangeFiles code="P6700LV" change={80481} />);
    expect(mocks.focus).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: /改动/ }));
    const file = await screen.findByRole("button", { name: /P\.spec/ });
    expect(mocks.focus).toHaveBeenCalledWith("change_files", {
      code: "P6700LV",
      change: 80481,
    });
    // The commit message is open from the start.
    const message = await screen.findByRole("article", { name: "提交说明" });
    expect(within(message).getByText("PASS")).toBeTruthy();
    await user.click(file);
    const view = await screen.findByRole("region", { name: "差异" });
    expect(within(view).getByText("SW = V15")).toBeTruthy();
    await waitFor(() =>
      expect(mocks.focus).toHaveBeenCalledWith("change_diff", {
        code: "P6700LV",
        change: 80481,
        path: "buildspec/P.spec",
      }),
    );
  });

  it("reads the team's commit layout", () => {
    const m = parseMessage(
      [
        "[HM][P6700LV][SPM][Feature]:自动化修改P6700LV.spec",
        "",
        "describe:本提交为自动生成",
        "触发链接：http://192.168.11.103:8080/job/AutoSpec/160/",
        "",
        "ST-result:[PASS]",
        "",
        "Associated Branch Path:",
        "build/buildplus",
        "",
        "是否影响共分支项目：否",
        "",
        "Change-Id: Iafe3d08",
        "Signed-off-by: shitianchi <tianchi.shi@sprocomm.com>",
      ].join("\n"),
    );
    expect(m.tags).toEqual(["HM", "P6700LV", "SPM", "Feature"]);
    expect(m.title).toBe("自动化修改P6700LV.spec");
    expect(m.fields).toEqual([
      { key: "describe", value: "本提交为自动生成" },
      {
        key: "触发链接",
        value: "http://192.168.11.103:8080/job/AutoSpec/160/",
      },
      { key: "ST-result", value: "[PASS]" },
      { key: "Associated Branch Path", value: "build/buildplus" },
      { key: "是否影响共分支项目", value: "否" },
    ]);
    expect(m.trailers.map((t) => t.key)).toEqual([
      "Change-Id",
      "Signed-off-by",
    ]);
  });

  it("shows each Gerrit status as its own pill", () => {
    render(
      <>
        <ChangeStatus status="MERGED" />
        <ChangeStatus status="ABANDONED" />
        <ChangeStatus status="NEW" />
      </>,
    );
    expect(screen.getByText("已合入").className).toContain("status-merged");
    expect(screen.getByText("已放弃").className).toContain("status-abandoned");
    expect(screen.getByText("待评审").className).toContain("status-new");
  });
});
