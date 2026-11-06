// Isolated, development-only design preview. All records here are synthetic.
import { createRoot } from "react-dom/client";
import ReleaseChecklist from "./ReleaseChecklist";
import { BrandMark } from "./components/brand-mark";
import { Badge } from "./components/ui/badge";
import { setLanguage } from "./i18n";
import {
  releaseStorageKey,
  writeReleaseChecks,
  type CheckEntry,
} from "./release-checklist";
import "./styles.css";

const code = "CHECKLIST-DEMO";
const sample = (
  status: CheckEntry["status"],
  owner: string,
  note: string,
  evidence = "",
): CheckEntry => ({
  status,
  owner,
  note,
  evidence,
  updatedAt: "2026-11-04T02:00:00Z",
});
async function preview() {
  await setLanguage("zh-CN");
  if (!localStorage.getItem(releaseStorageKey(code)))
    writeReleaseChecks(code, [
      {
        id: "demo-ota1",
        name: "OTA1 · Pre1",
        baseline: "V05 · build #94",
        branch: "DEMO_SYS_A16_MP / DEMO_VND_MP",
        target: "2026-11-20",
        createdAt: "2026-11-04T01:00:00Z",
        entries: {
          scope: sample(
            "passed",
            "项目负责人",
            "产品需求、客户反馈与内部必解清单已完成评审。新增范围通过变更评审后进入本轮。",
            "DEMO-SCOPE-01 · 需求评审纪要",
          ),
          branch: sample(
            "passed",
            "研发负责人",
            "沿用 MP 分支，已评估 SYS / VND 差异和功能依赖。",
            "DEMO-BRANCH-02 · 分支评审",
          ),
          common: sample(
            "blocked",
            "平台负责人",
            "相机平台修复仍待交付日期，确认后补齐专项验证窗口。",
            "DEMO-ISSUE-03 · 平台问题记录",
          ),
          certification: sample(
            "passed",
            "GMS 负责人",
            "已确认认证路径及版本基线，测试窗口已预留。",
            "DEMO-GMS-04 · 基线核对记录",
          ),
        },
      },
    ]);
  createRoot(document.getElementById("root")!).render(
    <div className="app-shell" style={{ height: "auto", minHeight: "100vh" }}>
      <header className="appbar" style={{ justifyContent: "space-between" }}>
        <span className="wordmark">
          <BrandMark />
          Maon
        </span>
        <Badge variant="outline">交互预览 · 示例数据</Badge>
      </header>
      <main
        className="page-content"
        style={{
          width: "100%",
          maxWidth: 1280,
          marginInline: "auto",
          padding: "28px clamp(16px, 4vw, 48px)",
        }}
      >
        <div
          style={{
            fontSize: 12,
            color: "var(--muted-foreground)",
            marginBottom: 24,
          }}
        >
          DEMO-A16 / 发布检查
        </div>
        <ReleaseChecklist code={code} branches={["DEMO_SYS_A16_MP"]} />
      </main>
    </div>,
  );
}
if (import.meta.env.DEV) void preview();
