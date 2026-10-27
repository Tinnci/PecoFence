#!/usr/bin/env python3
"""Deterministic CI watch contract/demo fixtures; no remote reads.

The demo scenario matches the CI/CD design: changes 18234, 18240, 18198 and 18177,
builds #913, #914, #482, #57 and the scheduled release #58. Stage labels and notes are
written the way SPM derives them from the same observations.
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NOW = "2026-10-27T06:21:00Z"


def build(job, number, state, elapsed=0, estimate=None, started=None, eta=None,
          overdue=False, commits=None, trigger=None, node=None, queue_reason=None):
    remaining = (estimate - elapsed) * 60000 if eta else None
    return dict(
        job=job,
        number=number,
        url_origin="jenkins-work",
        path="/" + "/".join("job/" + p for p in job.split("/")) + (f"/{number}" if number else ""),
        state=state,
        started_at=started,
        elapsed_ms=elapsed * 60000,
        estimated_ms=estimate * 60000 if estimate else None,
        eta_at=eta,
        remaining_ms=remaining,
        overdue=overdue,
        queue_reason=queue_reason,
        node=node,
        trigger=trigger or {"kind": "user", "id": "tianchi"},
        commit_ids=commits or [],
    )


STATES = dict(s="success", f="failure", u="unstable", a="aborted", r="running")


def history(job, job_numbers, codes, starts, commits=None):
    """Oldest first. `commits` maps a build number to its changeSets commit ids."""
    commits = commits or {}
    marks = []
    for n, c, t in zip(job_numbers, codes, starts):
        marks.append(dict(number=n, state=STATES[c], started_at=t,
                          commit_ids=commits.get(n, []),
                          url_origin="jenkins-work",
                          path="/" + "/".join("job/" + p for p in job.split("/")) + f"/{n}"))
    return marks


def log_window(text_lines, length):
    """Exactly `length` ASCII bytes of console text, padded with plausible build output."""
    body = "".join(line + "\n" for line in text_lines)
    filler = "[INFO] Tests run: 412, Failures: 0, Errors: 0, Skipped: 3\n"
    while len(body) < length:
        body += filler
    return body[:length]


def snapshot():
    daily = build("orion/launcher-daily-userdebug", 482, "running", 21, 33,
                  "2026-10-27T06:00:00Z", "2026-10-27T06:33:00Z", commits=["c" * 40],
                  trigger={"kind": "timer"}, node="node-07")
    daily_prev = build("orion/launcher-daily-userdebug", 481, "success", 31, 31,
                       "2026-10-26T22:00:00Z", commits=["8" * 40], trigger={"kind": "timer"})
    verify_fail = build("orion/launcher-gerrit-verify", 913, "failure", 16, 18,
                        "2026-10-27T04:00:00Z", commits=["9" * 40, "a" * 40],
                        trigger={"kind": "gerrit", "change": 18234, "patchset": 3}, node="node-04")
    verify_run = build("orion/launcher-gerrit-verify", 914, "running", 10, 18,
                       "2026-10-27T06:11:00Z", "2026-10-27T06:29:00Z", commits=["b" * 40],
                       trigger={"kind": "gerrit", "change": 18240, "patchset": 5}, node="node-04")
    release_last = build("release/ir-1113-full", 57, "success", 102, 102,
                         "2026-10-26T22:10:00Z", commits=["9" * 40], trigger={"kind": "timer"})
    system = build("orion/systemui-verify", 1167, "running", 24, 18, "2026-10-27T05:57:00Z",
                   overdue=True, node="node-03")
    queue = build("orion/carmode-lc3-smoke", None, "queued", 4, started="2026-10-27T06:17:00Z",
                  queue_reason="等待可用执行器 (label: arm64-large)")
    queue["started_at"] = "2026-10-27T06:17:00Z"

    # Histories are oldest first, 20 builds each; the newest entry equals the job's `last` build.
    daily_starts = [f"2026-10-{d:02d}T22:00:00Z" for d in range(8, 27)] + ["2026-10-27T06:00:00Z"]
    daily_hist = history("orion/launcher-daily-userdebug", range(463, 483),
                         "ssssssfssssssssfsssr", daily_starts[-20:],
                         {n: ["9" * 40] for n in range(463, 482)} | {482: ["c" * 40]})
    verify_starts = ["2026-10-%02dT09:00:00Z" % d for d in range(8, 27)] + ["2026-10-27T06:11:00Z"]
    verify_hist = history("orion/launcher-gerrit-verify", range(895, 915),
                          "ssfsssfssssfssssssfr", verify_starts[-20:],
                          {n: ["9" * 40] for n in range(895, 912)}
                          | {912: ["9" * 40], 913: ["9" * 40, "a" * 40], 914: ["9" * 40, "b" * 40]})
    release_hist = history("release/ir-1113-full", range(38, 58), "sssssssussssssssssss",
                           [f"2026-10-{8 + i:02d}T22:10:00Z" for i in range(20)],
                           {n: ["9" * 40] for n in range(38, 57)} | {57: ["9" * 40]})
    system_hist = history("orion/systemui-verify", range(1148, 1168), "sssssssssssssssssssr",
                          ["2026-10-27T05:57:00Z"] * 20)

    def job(entry, role, last, hist, median, mine=False, notify=False, health="ok"):
        return dict(id="job:" + entry, job=entry, role=role, only_mine=mine,
                    notify_on_failure=notify, last=last, history=hist,
                    median_ms=median * 60000, health=health)

    jobs = [
        job("orion/launcher-daily-userdebug", "daily", daily, daily_hist, 31, notify=True),
        job("orion/launcher-gerrit-verify", "verify", verify_run, verify_hist, 18, mine=True, notify=True),
        job("release/ir-1113-full", "release", release_last, release_hist, 102),
        job("orion/systemui-verify", "verify", system, system_hist, 18, notify=True),
    ]

    def change(n, subject, ps, rev, ci, labels, pipeline, note, note_state, mergeable=True,
               status="open"):
        return dict(number=n, subject=subject, owner="tianchi", patchset=ps, revision=rev,
                    status=status, labels=labels, mergeable=mergeable, ci=ci,
                    pipeline=pipeline, note=note, note_state=note_state,
                    url_origin="gerrit-work", path=f"/c/{n}")

    def stage(kind, state, label, at=None):
        return dict(kind=kind, state=state, label=label, entered_at=at)

    c18234 = change(
        18234, "Keep the Search box visible in Simple Mode", 3, "a" * 40, verify_fail,
        {"Verified": -1, "Code-Review": 1},
        [stage("submit", "done", "PS3", "2026-10-27T03:58:00Z"),
         stage("verify", "failed", "× #913", "2026-10-27T04:00:00Z"),
         stage("review", "pending", "CR+1"),
         stage("merge", "pending", "—"),
         stage("daily_build", "pending", "—"),
         stage("release", "pending", "—")],
        "verify #913 失败 · 上次成功 #912", "failed")
    c18240 = change(
        18240, "Rename the Home Settings entry", 5, "b" * 40, verify_run,
        {"Code-Review": 2},
        [stage("submit", "done", "PS5", "2026-10-27T05:50:00Z"),
         stage("verify", "running", "#914 约 8 分", "2026-10-27T06:11:00Z"),
         stage("review", "done", "CR+2"),
         stage("merge", "pending", "待验证"),
         stage("daily_build", "pending", "—"),
         stage("release", "pending", "—")],
        "#914 约 8 分后完成 · 验证通过后即可合入", None)
    c18198 = change(
        18198, "Support the LC3 audio codec in Car Mode", 2, "c" * 40, None,
        {"Verified": 1, "Code-Review": 2},
        [stage("submit", "done", "PS2", "2026-10-25T09:00:00Z"),
         stage("verify", "done", "V+1"),
         stage("review", "done", "CR+2"),
         stage("merge", "done", "已合入"),
         stage("daily_build", "running", "#482 约 12 分", "2026-10-27T06:00:00Z"),
         stage("release", "pending", "待进版本")],
        "已在 daily #482 中，约 12 分后完成", None, status="merged")
    c18177 = change(
        18177, "Restore the classic dock layout", 7, "d" * 40, None,
        {"Verified": 1, "Code-Review": 2},
        [stage("submit", "done", "PS7", "2026-10-26T13:00:00Z"),
         stage("verify", "done", "V+1"),
         stage("review", "done", "CR+2"),
         stage("merge", "warn", "⇄ 冲突"),
         stage("daily_build", "pending", "—"),
         stage("release", "pending", "—")],
        "与基线冲突，需变基", "warn", mergeable=False)

    branches = [
        dict(id="branch:orion/launcher:master", project="orion/launcher", branch="master",
             only_mine=False, include_merged=False, changes=[c18234, c18240], truncated=False),
        dict(id="branch:orion/launcher:release/ir-1113", project="orion/launcher",
             branch="release/ir-1113", only_mine=False, include_merged=True,
             changes=[c18198, c18177], truncated=False),
        dict(id="branch:orion/systemui:master", project="orion/systemui", branch="master",
             only_mine=True, include_merged=False, changes=[], truncated=False),
    ]

    release_train = [
        dict(job="orion/launcher-daily-userdebug", role="daily", current=daily,
             last_finished=daily_prev, scheduled_at=None, pending_merges=0, pending_mine=0,
             recent=dict(passed=8, total=9), median_ms=31 * 60000),
        dict(job="release/ir-1113-full", role="release", current=None, last_finished=release_last,
             scheduled_at="2026-10-27T14:00:00Z", pending_merges=1, pending_mine=1,
             recent=dict(passed=9, total=10), median_ms=102 * 60000),
    ]
    kpis = dict(submit_to_merge_median_ms=None, merge_to_release_median_ms=None,
                verify_pass_7d=dict(passed=6, total=7), watched_total=4, watched_failing=1)
    milestones = [
        dict(kind="code_freeze", label="代码冻结", at="2026-11-06T00:00:00Z"),
        dict(kind="release", label="IR 发布", at="2026-11-13T00:00:00Z"),
    ]
    last_success = dict(verify_hist[-3]) if verify_hist[-3]["state"] == "success" else None
    triage = dict(
        build=verify_fail,
        last_success=last_success,
        steps=[],
        log=dict(
            text=log_window([
                "SearchBoxTest > keepsVisibleInSimpleMode FAILED",
                "    expected: visible",
                "    but was: gone",
                "    at SearchBoxTest.kt:88",
                "SearchBoxTest > survivesRotation FAILED",
                "    at SearchBoxTest.kt:131",
                "2 tests failed, 412 passed",
            ], 1206912 - 1204880),
            start=1204880, end=1206912, total=3100000,
        ),
    )

    my_builds = [daily, system, queue, verify_fail, verify_run]
    return dict(
        observed_through=NOW,
        user=dict(jenkins_user="tianchi", gerrit_user="tianchi"),
        my_builds=my_builds,
        jobs=jobs,
        branches=branches,
        sources=[
            dict(source="jenkins", observed_at="2026-10-27T06:20:30Z", health="ok", error=None),
            dict(source="gerrit", observed_at="2026-10-27T06:19:00Z", health="ok", error=None),
        ],
        release_train=release_train,
        kpis=kpis,
        milestones=milestones,
        triage=triage,
    )


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, ensure_ascii=False, sort_keys=True, indent=2) + "\n")


def main():
    data = snapshot()
    root = ROOT / "crates/spm-contracts/fixtures/ci_watch"
    write(root / "snapshot.json", data)
    write(ROOT / "crates/plugin-spm/fixtures/ci-watch-demo.json", data)
    mutations = [
        ("history-bound", lambda d: d["jobs"][0]["history"].extend([d["jobs"][0]["history"][0]] * 2)),
        ("changes-bound", lambda d: d["branches"][0].update(changes=[d["branches"][0]["changes"][0]] * 21)),
        ("wrong-patchset", lambda d: d["branches"][0]["changes"][0].update(revision="e" * 40, patchset=4)),
        ("queued-number", lambda d: d["my_builds"][2].update(number=99)),
        ("raw-navigation", lambda d: d["my_builds"][0].update(path="https://evil.example/build")),
        ("pipeline-order", lambda d: d["branches"][0]["changes"][0]["pipeline"].reverse()),
        ("release-train-role", lambda d: d["release_train"][0].update(role="verify")),
        ("log-window-bytes", lambda d: d["triage"]["log"].update(end=d["triage"]["log"]["end"] + 1)),
        ("triage-not-failure", lambda d: d["triage"]["build"].update(state="success")),
    ]
    for name, mutate in mutations:
        d = json.loads(json.dumps(data))
        mutate(d)
        write(root / ("invalid-" + name + ".json"), d)


if __name__ == "__main__":
    main()
