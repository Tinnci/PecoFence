# Noto Serif SC / Noto Sans SC (subset)

The warm-paper note, TODO and calendar widgets title in **Noto Serif SC SemiBold**
and set body text in **Noto Sans SC Regular**. Both are SIL Open Font License 1.1
(`OFL.txt`); copyright Adobe, from Google's Noto CJK project.

| File | Upstream source | Version |
| --- | --- | --- |
| `PecoNotoSansSC-Regular.otf` | `Sans/SubsetOTF/SC/NotoSansSC-Regular.otf` | 2.004 |
| `PecoNotoSerifSC-SemiBold.otf` | `Serif/SubsetOTF/SC/NotoSerifSC-SemiBold.otf` | 2.003 |

Upstream: <https://github.com/notofonts/noto-cjk>. The files here are Modified
Versions under the OFL: `scripts/subset-widget-fonts.py` keeps Basic Latin, Latin-1,
general punctuation, CJK symbols, full-width forms, the GB2312 symbol rows and the
3,755 level-1 GB2312 hanzi (4,755 code points), and drops vertical metrics. The
font names are unchanged; Noto declares no Reserved Font Name.

Packaging copies these files and `OFL.txt` to `fonts/` beside `pecofence.exe`
(`FONT_ASSETS` in `scripts/package_desktop.py`). `pecofence_platform::fonts` loads
them into a private DirectWrite collection without installing them; characters
outside the subset, or a missing `fonts/` folder, fall back to the system fonts.
