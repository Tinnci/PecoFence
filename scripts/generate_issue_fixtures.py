#!/usr/bin/env python3
"""Deterministic issue-tracker wire fixtures for spm-contracts; no remote reads.

The scenario is the Orion IPD demo from the issues design: R-102 proposed to Gale from
Jordan's numbered list and Gale's reply, R-099 in conflict between Meegle and Lark, the
D-2291 severe defect, K-014 risk and the TR5 entry criteria on 2026-11-06.
Output is canonical JSON (sorted keys, compact, trailing newline) so the Rust golden tests
can compare bytes. Run from the repository root: python3 scripts/generate_issue_fixtures.py
"""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "crates" / "spm-contracts" / "fixtures"
OBSERVED = "2026-10-27T06:21:00Z"
TR5 = "2026-11-06T00:00:00Z"
PROJECT = "orion-ir"


def canon(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n"


def write(rel, value):
    path = ROOT / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(canon(value), encoding="utf-8")


def person(entity, name, relations, persona=None, title=None, handles=()):
    return {
        "entity_id": entity,
        "display_name": name,
        "persona": persona,
        "title": title,
        "relations": relations,
        "handles": [{"source": s, "value": v} for s, v in handles],
    }


GALE = person(
    "entity:gale",
    "Gale",
    ["development_representative", "proposed_owner"],
    persona="Launcher 组长",
    title="系统软件部",
    handles=[("lark", "ou_gale"), ("gerrit", "gale")],
)
JORDAN = person(
    "entity:jordan",
    "Jordan",
    ["pdt_manager"],
    title="Orion IR 项目协调",
    handles=[("lark", "ou_jordan"), ("meegle", "jordan")],
)
KAI = person(
    "entity:kai",
    "Kai",
    ["mentioned"],
    persona="Launcher 搜索",
    handles=[("lark", "ou_kai"), ("gerrit", "kai")],
)


def r102_item():
    return {
        "id": "R-102",
        "kind": "AR",
        "title": "Move the Search box to the bottom",
        "status": "待确认",
        "owner": None,
        "assignment": "proposed",
        "sources": ["meegle", "lark"],
        "due_at": None,
        "overdue": False,
        "queues": ["needs_my_reply", "awaiting_assignment_confirmation"],
        "trace": {"parents": ["SR-031"], "children": []},
    }


def r102_detail():
    return {
        "item": r102_item(),
        "gate_impact": {
            "next_gate": "tr5",
            "di_contribution": 0.0,
            "criteria": ["ar_owner_confirmed"],
        },
        "messages": [
            {
                "record_id": "lark:msg:0a1",
                "revision": 1,
                "occurred_at": "2026-10-26T01:41:00Z",
                "author": "Jordan",
                "excerpt": "IR 剩余四项，请认领：1 R-101 2 R-104 3 R-102 4 R-099",
                "reasons": ["token"],
                "precision": "exact",
                "participants": [],
                "missing_context": False,
            },
            {
                "record_id": "lark:msg:0a2",
                "revision": 1,
                "occurred_at": "2026-10-26T02:20:00Z",
                "author": "Gale",
                "excerpt": "1 Casey 3 我 4 Morgan",
                "reasons": ["token", "reply_chain"],
                "precision": "exact",
                "reply_to": "lark:msg:0a1",
                "participants": [],
                "missing_context": False,
            },
        ],
        "proposals": [
            {
                "id": "proposal:R-102:gale",
                "item_id": "R-102",
                "assignee": "Gale",
                "basis": "explicit_statement",
                "status": "proposed",
                "statement_author": "Gale",
                "evidence": [
                    {"record_id": "lark:msg:0a1", "revision": 1, "role": "request"},
                    {"record_id": "lark:msg:0a2", "revision": 1, "role": "reply"},
                ],
                "authority": "unknown",
                "acceptance": "unknown",
                "statement_at": "2026-10-26T02:20:00Z",
            }
        ],
        "people": [JORDAN, GALE, KAI],
        "who_to_ask": [
            {
                "person": KAI,
                "score": 0.82,
                "reasons": ["changes_in_area", "mentioned_in_thread"],
                "count": 3,
            },
        ],
        "suggestions": [
            {
                "kind": "related_discussion",
                "origin": "semantic",
                "subject_key": "issue:R-102",
                "object_key": "record:lark:msg:0b7",
                "title": "Kai 10-19 提到 Pad 横屏裁切",
                "similarity": 0.66,
                "reasons": ["discussion_in_area"],
            },
            {
                "kind": "who_to_ask",
                "origin": "people_who",
                "subject_key": "issue:R-102",
                "object_key": "person:entity:kai",
                "title": "Kai 做过底部布局原型",
                "similarity": None,
                "reasons": ["changes_in_area"],
            },
            {
                "kind": "missing_ipd_info",
                "origin": "rule",
                "subject_key": "issue:R-102",
                "title": "R-102 没有验收标准",
                "missing": "acceptance_criteria",
                "reference_id": "SR-031",
            },
        ],
        "suggestions_dismissed": 1,
        "truncated": False,
    }


def gate_readiness():
    track = [
        {"gate": "cdcp", "phase": "concept", "state": "passed"},
        {"gate": "pdcp", "phase": "plan", "state": "passed"},
        {"gate": "tr1", "phase": "development", "state": "passed"},
        {"gate": "tr2", "phase": "development", "state": "passed"},
        {"gate": "tr3", "phase": "development", "state": "passed"},
        {"gate": "tr4", "phase": "development", "state": "passed"},
        {"gate": "tr5", "phase": "verification", "state": "current", "due_at": TR5},
        {"gate": "tr6", "phase": "verification", "state": "upcoming"},
        {"gate": "adcp", "phase": "release", "state": "upcoming"},
    ]
    criteria = [
        {
            "kind": "di_threshold",
            "value": 14.5,
            "threshold": 10.0,
            "pass": False,
            "contributing": ["D-2291", "D-2287", "D-2279"],
        },
        {
            "kind": "no_open_fatal_or_major",
            "value": 2.0,
            "threshold": 0.0,
            "pass": False,
            "contributing": ["D-2291", "D-2300"],
        },
        {
            "kind": "legacy_approved",
            "value": 3.0,
            "threshold": 0.0,
            "pass": False,
            "contributing": ["D-2287", "D-2271", "D-2268"],
        },
        {
            "kind": "ar_owner_confirmed",
            "value": 4.0,
            "threshold": 0.0,
            "pass": False,
            "contributing": ["R-099", "R-101", "R-102", "R-104"],
        },
        {
            "kind": "high_risks_closed_or_accepted",
            "value": 1.0,
            "threshold": 0.0,
            "pass": False,
            "contributing": ["K-014"],
        },
    ]
    return {
        "observed_through": OBSERVED,
        "track": track,
        "next_gate": "tr5",
        "next_gate_due": TR5,
        "criteria": criteria,
        "ready": False,
        "di": {
            "value": 14.5,
            "weights": {"fatal": 10.0, "major": 3.0, "normal": 1.0, "hint": 0.1},
            "by_severity": [
                {"severity": "major", "count": 2, "contribution": 6.0},
                {"severity": "normal", "count": 8, "contribution": 8.0},
                {"severity": "hint", "count": 5, "contribution": 0.5},
            ],
        },
        "contributing": [
            "D-2291",
            "D-2300",
            "D-2287",
            "D-2279",
            "D-2271",
            "D-2268",
            "R-099",
            "R-101",
            "R-102",
            "R-104",
            "K-014",
        ],
    }


def what_if():
    return {
        "gate": "tr5",
        "start_di": 14.5,
        "threshold": 10.0,
        "steps": [
            {
                "item_id": "D-2291",
                "severity": "major",
                "di_reduction": 3.0,
                "effort_hours": 8.0,
                "di_after": 11.5,
            },
            {
                "item_id": "D-2287",
                "severity": "normal",
                "di_reduction": 1.0,
                "effort_hours": 3.0,
                "di_after": 10.5,
            },
            {
                "item_id": "D-2279",
                "severity": "normal",
                "di_reduction": 1.0,
                "effort_hours": 4.0,
                "di_after": 9.5,
            },
        ],
        "reaches_threshold": True,
        "unknown_effort": 11,
    }


def search_response():
    snippet = "把搜索框移到底部 Dock 上方；平板横屏时保持完整显示"
    term = "搜索框"
    # Spans are UTF-8 byte offsets into the snippet.
    start = len(snippet[: snippet.index(term)].encode("utf-8"))
    return {
        "mode_requested": "hybrid",
        "mode_used": "hybrid",
        "truncated": False,
        "results": [
            {
                "kind": "issue",
                "id": "R-102",
                "title": "Move the Search box to the bottom",
                "snippet": snippet,
                "channels": ["lexical", "semantic"],
                "score": 0.0325,
                "lexical": {
                    "rank": 1,
                    "spans": [{"start": start, "end": start + len(term.encode("utf-8"))}],
                },
                "semantic": {
                    "rank": 2,
                    "similarity": 0.71,
                    "span": {"start": 0, "end": len(snippet.encode("utf-8"))},
                },
                "why": ["exact_term", "same_meaning"],
                "linked_issue": "R-102",
            }
        ],
    }


def judge_request():
    return {
        "method": "judge_suggestion",
        "params": {
            "project_id": PROJECT,
            "kind": "duplicate_candidate",
            "subject_key": "defect:D-2301",
            "object_key": "defect:D-2291",
            "verdict": "dismiss",
            "note": None,
        },
    }


def search_request():
    return {
        "method": "search_workspace",
        "params": {
            "project_id": PROJECT,
            "query": "搜索框 底部",
            "mode": "hybrid",
            "scope": "all",
            "limit": 20,
        },
    }


def review_request():
    return {
        "method": "review_assignment",
        "params": {
            "project_id": PROJECT,
            "proposal_id": "proposal:R-102:gale",
            "verdict": "local_only",
            "note": None,
        },
    }


def query_request():
    return {
        "method": "query_issues",
        "params": {"project_id": PROJECT, "queue": "needs_my_reply", "kind": None, "limit": 50},
    }


def main():
    write("issues/query-issues-request.json", query_request())
    write("issues/get-issue-r102.json", r102_detail())
    write("issues/gate-readiness-tr5.json", gate_readiness())
    write("issues/gate-whatif-tr5.json", what_if())
    write("issues/search-hybrid.json", search_response())
    write("issues/judge-duplicate-request.json", judge_request())
    write("issues/search-request.json", search_request())
    write("issues/review-assignment-request.json", review_request())

    # Negative fixtures: each breaks exactly one rule.
    dts_no_severity = dict(r102_item(), id="D-2291", kind="DTS")
    write("issues/invalid-dts-without-severity.json", dts_no_severity)
    bad_id = dict(r102_item(), id="r-102")
    write("issues/invalid-issue-id.json", bad_id)
    judged_who = {
        "method": "judge_suggestion",
        "params": {
            "project_id": PROJECT,
            "kind": "who_to_ask",
            "subject_key": "issue:R-102",
            "object_key": "person:entity:kai",
            "verdict": "accept",
        },
    }
    write("issues/invalid-judge-who-to-ask.json", judged_who)
    zero_limit = {
        "method": "query_issues",
        "params": {"project_id": PROJECT, "queue": None, "kind": None, "limit": 0},
    }
    write("issues/invalid-query-limit.json", zero_limit)
    ready_lie = gate_readiness()
    ready_lie["ready"] = True
    write("issues/invalid-gate-ready-flag.json", ready_lie)
    wrong_steps = what_if()
    wrong_steps["steps"][1]["di_after"] = 10.0
    write("issues/invalid-whatif-arithmetic.json", wrong_steps)
    search = search_response()
    search["results"][0]["lexical"]["spans"][0]["end"] = 999
    write("issues/invalid-search-span.json", search)
    fallback_lie = search_response()
    fallback_lie["mode_used"] = "lexical"
    write("issues/invalid-search-fallback.json", fallback_lie)


if __name__ == "__main__":
    main()
