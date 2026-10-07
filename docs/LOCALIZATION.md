# Localization

The application supports live language switching. The native Reactor/WinUI 3
Settings language selector and localized forms are implemented; automated checks
do not replace visual and accessibility acceptance across the complete language/DPI
matrix. New installations follow the Windows **display
language**, not the region/date-format setting.
Existing configurations without a `language` field retain Simplified Chinese.

| Setting | Language |
|---|---|
| `system` | Follow Windows; unsupported display languages fall back to English |
| `zh-CN` | Simplified Chinese |
| `zh-TW` | Traditional Chinese |
| `en` | English |
| `ja` | Japanese |
| `ko` | Korean |
| `de` | German |
| `fr` | French |
| `es` | Spanish |
| `pt-BR` | Portuguese (Brazil) |
| `ru` | Russian |

Chinese display locales using Traditional Chinese resolve to `zh-TW`. Portuguese
display locales use the supported Brazilian Portuguese catalog.

## Catalogs

`locales/*.json` use the original Simplified Chinese application message as a
stable key. Simplified Chinese uses the source text directly. Keep keys intact;
edit the value to improve a translation.

Native code uses `pecofence_core::i18n::text` and `i18n::format`; the Reactor/WinUI 3
Settings integration uses the same catalog source. Catalogs are
embedded into the executable; installation does not require copying a locale
directory or frontend scripts.

Numbered placeholders (`{0}`, `{1}`, etc.) may be reordered for grammar but must
be preserved. `%1` is a Windows drag-description insertion marker and must also
remain intact. Arguments are substituted once; braces inside a filename are literal.

Control labels and dynamic status messages explicitly use the catalog helpers.
Never translate user filenames, rule names, snapshot names or fence titles.
Language changes must preserve native form drafts and selection.

Default group/rule names are localized when created. They are then saved as names
and are not rewritten by later language changes.

## Validation and adding a language

Run `python scripts/check-locales.py` and the core tests after any catalog edit.
The checker verifies source coverage and placeholder parity in every language.
Automated native Settings coverage exists, but does not prove the real Windows
presentation. Long translations, accessibility and mixed-DPI live language
switching require release acceptance across all ten languages; see
[verification gates](VERIFICATION_GATES.md).

To add a language, add its catalog, extend `Language`/`SUPPORTED` in
`crates/core/src/i18n.rs`, map the Windows language in `crates/platform/src/locale.rs`,
and update the settings selector and verification language lists.

Windows-owned dialogs, dates, file-type descriptions and third-party Explorer
menu entries are provided by Windows and retain the system's own language.

## README translations

The root `README.md` is English. Each interface language also has a README under
`docs/readme/README.<language>.md`, sharing the same structure and a localized hero
image in `docs/assets/hero-<language>.png`. When the English README changes, update
every translation in the same change, keep UI terms identical to the language's
catalog in `locales/`, and rerun `python scripts/make-readme-media.py --stills-only`
if the hero text in `HERO_TEXT` changed.
