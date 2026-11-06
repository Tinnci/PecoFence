// Development-only preview with synthetic jobs and no Jenkins requests.
import { createRoot } from "react-dom/client";
import {
  BuildActivity,
  RunningEstimate,
  DurationSummary,
} from "./BuildInsights";
import { BrandMark } from "./components/brand-mark";
import { Badge } from "./components/ui/badge";
import { setLanguage } from "./i18n";
import type { JobView } from "./contracts/setup";
import "./styles.css";
async function preview() {
  await setLanguage("zh-CN");
  const now = Date.now();
  const minute = 60000;
  const job: JobView = {
    job: "demo/DEMO_A16_MP",
    name: "DEMO_A16_MP",
    phase: "mp",
    disabled: false,
    parameters: {},
    changes: [],
    builds: [
      {
        number: 95,
        building: true,
        timestamp: now - 95 * minute,
        duration: 0,
        estimated: 130 * minute,
        node: "build-node-01",
      },
      ...[120, 135, 150, 125, 110].map((duration, i) => ({
        number: 94 - i,
        building: false,
        result: "SUCCESS",
        timestamp: now - (i + 1) * 86400000,
        duration: duration * minute,
        estimated: 130 * minute,
        node: "build-node-01",
      })),
    ],
  };
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
        style={{
          maxWidth: 1100,
          width: "100%",
          margin: "auto",
          padding: "32px clamp(16px, 4vw, 48px)",
        }}
      >
        <p className="quiet" style={{ marginBottom: 16 }}>
          DEMO-A16 / 构建
        </p>
        <h1 style={{ fontSize: 24, marginBottom: 24 }}>构建与排队</h1>
        <BuildActivity
          now={now}
          data={{
            code: "DEMO-A16",
            jobs: [job],
            computed_at: new Date(now).toISOString(),
            errors: [],
            queue: {
              capacity: {
                observed_at: new Date(now).toISOString(),
                slots: [
                  {
                    node: "build-node-01",
                    labels: ["fast_build_servers"],
                    online: true,
                    idle: false,
                    job: job.job,
                    started: now - 95 * minute,
                    low: 120 * minute,
                    high: 135 * minute,
                  },
                  {
                    node: "build-node-02",
                    labels: ["fast_build_servers"],
                    online: true,
                    idle: false,
                    job: "demo/OTHER_MP",
                    started: now - 5 * minute,
                    low: 45 * minute,
                    high: 55 * minute,
                  },
                ],
                tasks: [101, 102].map((id, i) => ({
                  id,
                  job: job.job,
                  label: "fast_build_servers",
                  since: now - (i ? 8 : 24) * minute,
                  blocked: false,
                  buildable: true,
                  concurrent: true,
                  low: 120 * minute,
                  high: 135 * minute,
                  samples: 5,
                })),
              },
              server_count: 6,
              observed_at: new Date(now).toISOString(),
              items: [
                {
                  id: 101,
                  job: job.job,
                  since: now - 24 * minute,
                  reason:
                    "Waiting for next available executor on ‘fast_build_servers’",
                  blocked: false,
                  buildable: true,
                },
                {
                  id: 102,
                  job: job.job,
                  since: now - 8 * minute,
                  reason:
                    "Waiting for next available executor on ‘fast_build_servers’",
                  blocked: false,
                  buildable: true,
                },
              ],
            },
          }}
        />
        <article
          style={{
            border: "1px solid var(--border)",
            borderRadius: 10,
            background: "var(--card)",
            padding: 22,
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              marginBottom: 16,
            }}
          >
            <h2>{job.name}</h2>
            <Badge variant="secondary">MP</Badge>
          </div>
          <RunningEstimate job={job} now={now} />
          <div style={{ marginTop: 18 }}>
            <DurationSummary job={job} />
          </div>
          <p className="quiet" style={{ marginTop: 6, fontSize: 11 }}>
            build-node-01 · 时间预估随实际构建更新
          </p>
        </article>
      </main>
    </div>,
  );
}
if (import.meta.env.DEV) void preview();
