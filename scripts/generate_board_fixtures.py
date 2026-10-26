"""Generate the synthetic Orion board fixtures for spm-contracts.

The scenario follows docs/EVIDENCE_BOARD_PROPOSAL.md. All people, projects,
accounts and records are fictional. Run from the repository root:

    python scripts/generate_board_fixtures.py
"""

from __future__ import annotations

import copy
import json
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "crates/spm-contracts/fixtures/board"

SESSION = "22222222-2222-4222-8222-222222222222"
LANE = {"project": "orion", "model": "orion-ir"}

PEOPLE = {
    name: {
        "person_id": f"p:{name}",
        "account": None,
        "display_name": name.capitalize(),
    }
    for name in ("taylor", "gale", "casey", "morgan")
}


def gerrit_person(name: str, account_id: str) -> dict:
    person = copy.deepcopy(PEOPLE[name])
    person["account"] = {"system": "gerrit", "tenant": "review.acme", "account_id": account_id}
    return person


GERRIT = {"gale": gerrit_person("gale", "1000412"), "morgan": gerrit_person("morgan", "1000231")}


def ref(system: str, project: str, kind: str, record_id: str, tenant: str = "acme") -> dict:
    return {
        "system": system,
        "tenant": tenant,
        "project": project,
        "record_kind": kind,
        "record_id": record_id,
    }


def story(number: int) -> dict:
    return ref("meegle", "orion", "story", f"8823{number}")


CHANGE = ref("gerrit", "launcher", "change", "4711", "review.acme")
PS1 = ref("gerrit", "launcher", "patchset", "4711/1", "review.acme")
PS2 = ref("gerrit", "launcher", "patchset", "4711/2", "review.acme")


def rel(
    rid: str,
    kind: str,
    person: dict,
    subject: dict,
    basis: str,
    source_time: str | None,
    observed_at: str,
    evidence: list[str],
    *,
    derivation: str | None = None,
    statement: dict | None = None,
    approval: dict | None = None,
    status: str = "current",
    supersedes: str | None = None,
) -> dict:
    return {
        "id": rid,
        "kind": kind,
        "person": person,
        "subject": subject,
        "basis": basis,
        "derivation": derivation,
        "status": status,
        "source_time": source_time,
        "observed_at": observed_at,
        "evidence": evidence,
        "supersedes": supersedes,
        "statement": statement,
        "approval": approval,
    }


LARK_SEEN = "2026-10-26T02:15:00Z"
GERRIT_SEEN = "2026-10-26T02:14:00Z"
GALE_SAYS = {"author": PEOPLE["gale"], "kind": "numbered_reply", "authority": "unknown"}
LARK_EVIDENCE = ["ev:l-request", "ev:l-reply"]


def lark_assignment(number: int, name: str) -> dict:
    return rel(
        f"rel-10{number}-lark",
        "assigned",
        PEOPLE[name],
        story(number),
        "explicit_statement",
        "2026-10-26T02:11:00Z",
        LARK_SEEN,
        LARK_EVIDENCE,
        statement=GALE_SAYS,
    )


def meegle_assignment(number: int, name: str) -> dict:
    return rel(
        f"rel-10{number}-meegle",
        "assigned",
        PEOPLE[name],
        story(number),
        "issue_field",
        "2026-10-27T06:00:00Z",
        "2026-10-27T06:05:00Z",
        [f"ev:m-8823{number}-owner"],
    )


def history(number: int) -> list[dict]:
    rows = [
        rel(
            f"rel-10{number}-history-morgan",
            "historical_contributor",
            PEOPLE["morgan"],
            story(number),
            "derived",
            "2026-10-14T03:20:00Z",
            GERRIT_SEEN,
            ["ev:g-4711-ps1"],
            derivation="history@1",
        )
    ]
    if number == 2:
        rows.append(
            rel(
                "rel-102-contact-morgan",
                "implementation_contact",
                PEOPLE["morgan"],
                story(2),
                "derived",
                "2026-10-14T03:20:00Z",
                GERRIT_SEEN,
                ["ev:g-4711-ps1"],
                derivation="contact-from-history@1",
            )
        )
    return rows


def change_4711() -> list[dict]:
    def vote(rid: str, subject: dict, patchset: int, carried: str) -> dict:
        return rel(
            rid,
            "approval",
            GERRIT["gale"],
            subject,
            "source_field",
            "2026-10-14T05:00:00Z",
            GERRIT_SEEN,
            [f"ev:g-4711-ps{patchset}-labels"],
            approval={
                "patchset": patchset,
                "label": "Code-Review",
                "value": 2,
                "carried_over": carried,
                "post_submit": False,
                "self_approval": False,
            },
        )

    return [
        rel("rel-4711-owner", "change_owner", GERRIT["morgan"], CHANGE, "source_field",
            "2026-10-14T03:20:00Z", GERRIT_SEEN, ["ev:g-4711"]),
        rel("rel-4711-ps1-author", "patchset_author", PEOPLE["morgan"], PS1, "source_field",
            "2026-10-14T03:20:00Z", GERRIT_SEEN, ["ev:g-4711-ps1"]),
        rel("rel-4711-ps2-uploader", "patchset_uploader", GERRIT["morgan"], PS2, "source_field",
            "2026-10-15T01:00:00Z", GERRIT_SEEN, ["ev:g-4711-ps2"]),
        rel("rel-4711-reviewer-gale", "reviewer_listed", GERRIT["gale"], CHANGE, "source_field",
            "2026-10-14T03:25:00Z", GERRIT_SEEN, ["ev:g-4711"]),
        vote("rel-4711-ps1-cr", PS1, 1, "no"),
        # The vote predates patch set 2's creation, so Gerrit copied it.
        vote("rel-4711-ps2-cr", PS2, 2, "inferred"),
        rel("rel-4711-submitter", "submitter", GERRIT["gale"], CHANGE, "source_field",
            "2026-10-15T01:30:00Z", GERRIT_SEEN, ["ev:g-4711"]),
    ]


def progress(code_linked: int) -> dict:
    return {
        "code": {
            "linked": code_linked,
            "merged": code_linked,
            "open": 0,
            "abandoned": 0,
            "relation": "candidate" if code_linked else "unrelated",
        },
        "build": "unknown",
        "verification": "not_started",
    }


TITLES = {
    1: "Remove the regional feature",
    2: "Move the Search box to the bottom",
    3: "Rename the Home Settings entry",
    4: "Add a Search box visibility switch",
}


def card(number: int, *, started: bool, relationships: list[dict], assignee: str | None,
         next_action: tuple[str, str] | None, observed: dict, extra_evidence: int = 0) -> dict:
    if started:
        workflow = {"source_state": "开发中", "node_key": "develop",
                    "source_time": "2026-10-27T06:00:00Z"}
        stage = {"stage": "implementation", "basis": "workflow_node", "overlays": [],
                 "raw_node": "develop", "explanation": "Meegle node develop in progress"}
    else:
        workflow = {"source_state": "待开发", "node_key": "backlog", "source_time": "2026-10-20T02:00:00Z"}
        stage = {"stage": "not_started", "basis": "workflow_node", "overlays": [],
                 "raw_node": "backlog", "explanation": "Meegle node backlog; no confirmed change"}
    linked = 1 if number in (1, 2) else 0
    if linked:
        stage["explanation"] += "; change 4711 is a candidate link"
    return {
        "card_id": f"meegle:acme/orion/story/8823{number}",
        "source": story(number),
        "business_number": f"ORION-R-10{number}",
        "title": TITLES[number],
        "swimlane": LANE,
        "workflow": workflow,
        "column": "doing" if started else "todo",
        "stage": stage,
        "started_at": "2026-10-27T06:00:00Z" if started else None,
        "schedule": {"due": "2026-11-06T00:00:00Z", "baseline_due": None, "basis": story(number)},
        "stage_history": [{"stage": stage["stage"], "from": workflow["source_time"], "to": None}],
        "event_marks": [],
        "current_assignee": assignee,
        "relationships": relationships,
        "progress": progress(linked),
        "blockers": [],
        "next_action": None if next_action is None
        else {"text": next_action[0], "owner": PEOPLE[next_action[1]]},
        "conflicts": [],
        "evidence_count": len({e for r in relationships for e in r["evidence"]}) + extra_evidence,
        "observed_through": observed,
    }


def sources(meegle_at: str, gerrit_at: str, people_at: str) -> list[dict]:
    def source(system: str, tenant: str, at: str, watermark: str) -> dict:
        return {
            "source": system,
            "tenant": tenant,
            "observed_at": at,
            "source_revision": None,
            "watermark": watermark,
            "coverage": "complete",
            "page_complete": True,
            "permission_complete": True,
            "freshness_ttl_secs": 900,
            "error": None,
        }

    return [
        source("meegle", "acme", meegle_at, meegle_at),
        source("gerrit", "review.acme", gerrit_at, gerrit_at),
        source("people", "acme", people_at, "changes:318"),
    ]


def milestones() -> list[dict]:
    return [{"id": "code-freeze", "label": "Code freeze", "at": "2026-11-06T00:00:00Z"},
            {"id": "ir-release", "label": "IR release", "at": "2026-11-13T00:00:00Z"}]


def snapshot(revision: int, computed_at: str, cards: list[dict], counts: dict, src: list[dict]) -> dict:
    return {
        "project_id": "orion",
        "delivery_scope_id": "orion-ir-2026-11",
        "baseline_id": None,
        "policy_revision": 3,
        "daemon_session": SESSION,
        "revision": revision,
        "computed_at": computed_at,
        "project_timezone": "Asia/Shanghai",
        "freshness_ttl_secs": 900,
        "sources": src,
        "lanes": [{"swimlane": LANE,
                   "counts": {"todo": 0, "doing": 0, "done": 0, "unclassified": 0, "blocked": 0} | counts}],
        "totals_complete": True,
        "cards": cards,
        "milestones": milestones(),
        "next_cursor": None,
    }


def after_reply() -> dict:
    observed = {"gerrit": GERRIT_SEEN, "meegle": "2026-10-26T02:12:00Z", "people": LARK_SEEN}
    cards = [
        card(1, started=False, assignee="rel-101-lark",
             relationships=[lark_assignment(1, "casey"), *change_4711(), *history(1)],
             next_action=("Record Casey as the Meegle owner; Gale's Lark assignment is not in Meegle", "taylor"),
             observed=observed),
        card(2, started=False, assignee=None, relationships=history(2),
             next_action=("Assign ORION-R-102: Gale's reply named nobody for task 2", "taylor"),
             observed=observed, extra_evidence=len(LARK_EVIDENCE)),
        card(3, started=False, assignee="rel-103-lark", relationships=[lark_assignment(3, "gale")],
             next_action=("Record Gale as the Meegle owner", "taylor"), observed=observed),
        card(4, started=False, assignee="rel-104-lark", relationships=[lark_assignment(4, "morgan")],
             next_action=("Record Morgan as the Meegle owner", "taylor"), observed=observed),
    ]
    return snapshot(41, "2026-10-26T02:20:00Z", cards, {"todo": 4},
                    sources("2026-10-26T02:12:00Z", GERRIT_SEEN, LARK_SEEN))


def after_meegle() -> dict:
    observed = {"gerrit": "2026-10-27T06:04:10Z", "meegle": "2026-10-27T06:05:00Z", "people": LARK_SEEN}
    cards = [
        card(1, started=True, assignee="rel-101-meegle",
             relationships=[lark_assignment(1, "casey"), meegle_assignment(1, "casey"),
                            *change_4711(), *history(1)],
             next_action=("Confirm whether change 4711 covers this requirement", "taylor"),
             observed=observed),
        card(2, started=True, assignee="rel-102-meegle",
             relationships=[meegle_assignment(2, "casey"), *history(2)],
             next_action=("Confirm whether change 4711 covers this requirement", "taylor"),
             observed=observed, extra_evidence=len(LARK_EVIDENCE)),
        card(3, started=False, assignee="rel-103-lark", relationships=[lark_assignment(3, "gale")],
             next_action=("Record Gale as the Meegle owner", "taylor"), observed=observed),
        card(4, started=False, assignee="rel-104-lark", relationships=[lark_assignment(4, "morgan")],
             next_action=("Record Morgan as the Meegle owner", "taylor"), observed=observed),
    ]
    return snapshot(57, "2026-10-27T06:10:00Z", cards, {"todo": 2, "doing": 2},
                    sources("2026-10-27T06:05:00Z", "2026-10-27T06:04:10Z", LARK_SEEN))


def evidence_r102() -> dict:
    def item(eid: str, source: dict, locator: str, at: str, seen: str, summary: str, rels: list[str]) -> dict:
        return {"id": eid, "source": source, "field_locator": locator, "source_time": at,
                "observed_at": seen, "summary": summary, "relationship_ids": rels}

    chat = ref("lark", "orion-ir-chat", "message", "om_request")
    reply = ref("lark", "orion-ir-chat", "message", "om_reply")
    return {
        "card_id": "meegle:acme/orion/story/88232",
        "revision": 57,
        "items": [
            item("ev:g-4711-ps1", PS1, "revisions.1.commit.author", "2026-10-14T03:20:00Z", GERRIT_SEEN,
                 "Change 4711 patch set 1 authored by Morgan; touches search-position configuration",
                 ["rel-102-history-morgan", "rel-102-contact-morgan"]),
            item("ev:l-request", chat, "content.text", "2026-10-26T02:01:00Z", LARK_SEEN,
                 "Taylor lists Orion IR tasks 1 to 4; task 2 is moving the Search box to the bottom", []),
            item("ev:l-reply", reply, "content.text", "2026-10-26T02:11:00Z", LARK_SEEN,
                 "Gale replies to that message: \"1 Casey 3 me 4 Morgan\"; task 2 is not named", []),
            item("ev:m-88232-owner", story(2), "workflow.nodes.develop.owners", "2026-10-27T06:00:00Z",
                 "2026-10-27T06:05:00Z", "Meegle develop node owner set to Casey", ["rel-102-meegle"]),
        ],
        "next_cursor": None,
    }


def write(out: Path, values: dict) -> None:
    out.mkdir(parents=True, exist_ok=True)
    for name, value in values.items():
        (out / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    write(OUT, {
        "orion-after-reply.json": after_reply(),
        "orion-after-meegle.json": after_meegle(),
        "orion-r102-evidence.json": evidence_r102(),
    })
    invalid_history = after_reply()
    invalid_history["cards"][0]["stage_history"] *= 2
    invalid_marks = after_reply()
    invalid_marks["cards"][0]["event_marks"] = [{"at": LARK_SEEN, "system": "lark", "evidence_id": "ev:l-reply"}] * 13
    write(OUT.parent / "invalid", {"board-stage-overlap.json": invalid_history,
                                    "board-event-marks-13.json": invalid_marks})
    import board_demo_fixtures

    write(board_demo_fixtures.OUT, board_demo_fixtures.build())


if __name__ == "__main__":
    main()
