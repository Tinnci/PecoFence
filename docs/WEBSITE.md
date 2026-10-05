# Website setup

The independent [Tinnci/PecoFence repository](https://github.com/Tinnci/PecoFence)
does not currently configure a public website deployment. No upstream domain,
Cloudflare project or account is assumed to belong to this edition. Choose a host
and URL before enabling publishing; see [INDEPENDENCE.md](INDEPENDENCE.md).

## Local build and previews

The site uses Python >=3.11 and the standard library, with no Node build toolchain:

```powershell
python scripts/build-site.py --strict
python -m http.server 8000 --directory dist/site
```

Open <http://localhost:8000>. If needed, select a compatible Python with
`uv run --no-project --python ">=3.11" python scripts/build-site.py --strict`.
The default local base URL is `http://localhost:8000`; `--base` overrides it for
previews, for example `--base http://localhost:9000`. `--strict` checks translation
keys against `site/i18n/en.json`.

`site/site.json` points to `https://github.com/Tinnci/PecoFence`, with `baseUrl`
and `customDomain` initially `null`. The build writes localized pages, assets,
Open Graph previews, `robots.txt` and `sitemap.xml` under `dist/site/`.
There are no Store/winget buttons or analytics beacon.

## Configure a public deployment

1. Choose a hosting account and create your own site/project.
2. Set `site/site.json`'s `baseUrl` to the actual public HTTPS URL, including any
   repository base path. For example, `https://<owner>.github.io/<repository>`
   is a **placeholder**, not an existing deployment.
3. Leave `customDomain` as `null` unless you control and configure a custom domain.
   If set, it must match the base URL hostname, and the URL must have no base path.
   Only this matching configuration generates a `CNAME`; local defaults do not.
4. Validate before uploading:

   ```powershell
   python scripts/build-site.py --strict --deploy
   ```

`--deploy` requires an explicit public HTTPS URL; localhost defaults are rejected.
The workflows do not pass `--base`, so configure `baseUrl` before enabling them.
An explicit `--base` override must also satisfy deployment validation when used
with `--deploy`; do not accidentally publish preview URLs as production metadata.

### Cloudflare Pages

Create a direct-upload Pages project in your own Cloudflare account. Configure
these repository Actions settings for `.github/workflows/website.yml`:

| Kind | Name | Value |
|---|---|---|
| Variable | `ENABLE_WEBSITE_DEPLOY` | `true` only when ready to publish |
| Variable | `CLOUDFLARE_PAGES_PROJECT` | Your actual Pages project name |
| Secret | `CLOUDFLARE_API_TOKEN` | Token with Account → Cloudflare Pages → Edit for your account |
| Secret | `CLOUDFLARE_ACCOUNT_ID` | Your account ID |

The workflow only publishes from `main` and calls `--strict --deploy`.
For a manual local upload, authenticate with `npx wrangler login`, build with the
same flags, then run:

```powershell
npx wrangler pages deploy dist/site --project-name "<your-project>" --branch main
```

`<your-project>` is a placeholder to replace. Configure any custom domain in your
own project's Custom domains settings and follow the provider's DNS instructions;
no existing domain, DNS zone or certificate is supplied by this repository.

### GitHub Pages alternative

GitHub Pages is an optional alternative, **not an already live fallback**.
Configure the repository's own Pages URL in `site/site.json` first. In
**Settings → Pages**, select **GitHub Actions** as the source, then set the
repository variable `ENABLE_GITHUB_PAGES=true`. Manually run
`.github/workflows/pages.yml` from `main`; it calls `--strict --deploy` and uses
GitHub Actions OIDC for deployment. Configure any custom domain and HTTPS in
repository Pages settings separately. Keep unused hosting workflows disabled.

## Copy and media

- `site/template.html` and `site/assets/site.css` / `site.js` define the page.
- Edit `site/i18n/en.json` first, then keep every language's keys in sync. Reuse
  native UI terms from `locales/` and localized README wording from `docs/readme/`.
  Only template strings inserted with `{{raw:...}}` accept the supported markup;
  other strings are escaped.
- Checked-in images and clips let the site build without the local promo project.
  `python scripts/make-site-media.py` regenerates media from the ignored
  `extras/pecofence-promo/public/` directory when that source is available.
  Replace inherited demonstrations with this edition's own demos when ready,
  while retaining applicable licensing and attribution.
