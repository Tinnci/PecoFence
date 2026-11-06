# Shared regional work calendars

Consumers: PecoFence widgets, SPM/Maon and visualizer-cli exports. This crate owns data and
workday exceptions; each application keeps its own rendering and planning rules.

- China: https://github.com/NateScarlet/holiday-cn (MIT), annual State Council notices,
  including make-up working dates. Raw notices retain their official `papers` links.
- India, Bangladesh, Brazil: https://github.com/vacanza/holidays (MIT), versioned public
  holiday rules with every supported state/subdivision. `status: calculated` distinguishes
  generated rules (including estimated lunar dates) from China's published arrangements.

`CalendarYear` carries country, subdivision, year, timezone, weekday baseline, dated
exceptions, source, revision and status. A weekday is 0=Monday…6=Sunday. An exception's
`is_off_day=false` takes precedence over the weekly baseline. Company overrides should be
applied last. National defaults are a baseline; choose the applicable state and company
schedule for local delivery commitments. BR uses São Paulo time as a display default;
projects in other timezones can override it.

Run `uv run scripts/update-work-calendars.py` from the repository root to regenerate the
snapshots. `--cached-china` reuses downloaded annual notices. Public JSON under `data/`
is usable by Python, Rust and JavaScript; `CalendarYear::visualizer()` exports the existing
`work_calendars` shape used by visualizer-cli. Shared runtime cache:
`%LOCALAPPDATA%/Tinnci/calendars/<country[-subdivision]>/<year>.json` (XDG state on Linux).
Rendering and ordinary reads use cached/bundled data. Explicit subscriptions refresh
validated payloads and retain the last usable year on network failure.

Other /dev tools (mail, issue/Gerrit/Jenkins readers and build runners) keep timestamps
without defining workday scheduling; they need no holiday dependency. The generic
planning-core accepts expanded working intervals from callers rather than downloading
country data itself.
