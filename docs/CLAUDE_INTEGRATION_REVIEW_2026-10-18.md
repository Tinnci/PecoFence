# Claude integration review — 2026-10-18

## Result

Integrate the two completed Claude branches into local `main` while retaining the existing native diagram widget.

- `claude/interesting-aryabhata-325d58`, `7a012ac`: center SPM button, grip and toolbar-pill labels; vertically align single-line text while retaining top-leading wrapped text.
- `claude/youthful-brahmagupta-79de28`, `638ddd7`: load bundled Noto Sans SC and Noto Serif SC subsets through a private DirectWrite collection; package the fonts and OFL license with both portable and MSIX payloads.
- Retain the diagram scene's `path` field when combining the SPM alignment change with the newer scene API.
- Retain both diagram and font release notes. The font branch's executable size is not a measurement of the combined build.

The widget redesign, shared pill controls, lunar calendar, paper palette and Fluent motion were already on local `main` before this integration. They are retained, rather than reapplied.

## Design choices

Keep native Windows rendering and the existing scene, input and accessibility boundaries. The desktop application does not acquire Node.js, a browser engine or a JavaScript runtime.

Use the paper palette and private Noto faces for paper-style widgets. Other application text keeps its existing font roles; missing bundled fonts fall back to system fonts. Fonts are loaded from the installation directory and are not installed system-wide. Font assets total 2,816,904 bytes, excluding the license.

Retain high-contrast color handling and the Windows animation preference. These code paths have automated coverage, but this integration is not on-device visual or assistive-technology acceptance.

The native Mermaid widget remains a limited flowchart/Gantt implementation. It does not embed Pele and does not claim full Mermaid compatibility. Styling, subgraphs and several Gantt directives are ignored as documented in `DIAGRAM_WIDGET.md`. Full compatibility and additional diagram types remain follow-up work.

The SPM alignment change operates on presentation scenes. It does not alter the public SPM contract, daemon data or private backend.

## Review fixes

Malformed Gantt duration text containing a multibyte suffix could panic during UTF-8 byte slicing. A valid duration applied beyond the maximum supported date could also panic. Both were reproduced by failing regression tests. Duration parsing now uses character-safe suffix handling, and date arithmetic returns a parse error on overflow.

Add native DirectWrite tests that load both actual bundled font assets, verify the configured family names, and measure CJK, Latin and an out-of-subset glyph. These tests require real font files; they do not silently skip a missing fixture.

## Validation

- Rust formatting and whitespace checks pass.
- Twelve source-policy, public-dependency, locale, README, packaging, runtime-tooling and CLI-guidance checks pass.
- Eight portable Rust packages pass 174 unit and documentation tests in total.
- The final widget package passes all 44 unit tests, including the two parser regressions and the paper-font role checks.
- Strict Windows-target Clippy passes for the entire workspace and all targets: `--target x86_64-pc-windows-msvc --all-targets -- -D warnings`.
- Two DirectWrite tests execute successfully on Windows with the actual bundled fonts in a temporary local Windows directory. Temporary fixtures are removed afterward. No fonts are installed.

An initial attempt to run the entire workspace on Linux failed in Windows-only dependencies. Portable tests were therefore scoped to the eight platform-independent packages; Windows-only code was checked with the Windows target. This is not a claim that the full Windows workspace test suite ran.

## Remaining acceptance

Before distributing a new build, run the full Windows verification and package gates, including the combined executable-size limit, binding regeneration, release provenance and a real portable archive. The earlier font-branch size measurement does not satisfy this requirement.

Native widget appearance, IME editing, screen readers, high contrast, reduced motion, DPI and multi-monitor behavior still need device acceptance. Neither Clippy nor the DirectWrite tests establishes those results.

No application installation, deployment, workspace migration or remote push is part of this integration. The original Claude worktrees are preserved.
