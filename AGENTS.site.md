# AGENTS.site.md — showcase site (`site/`)

Published to `https://rorganizer.loulouw-labs.fr` by
`.github/workflows/pages.yml` (GitHub Pages, source "GitHub Actions"). DNS:
`CNAME rorganizer → Loulouw.github.io`. Pages publishes `site/` as is:
anything put there is public.

## Hand-written HTML/CSS, no build, no dependency

Deliberate: the supply-chain rule forbids running unaudited build code, and
an npm chain or a site generator is exactly that. Source is output, so
there is no gap for bugs to hide in. Revisit beyond ~10 pages, when the
duplicated `<head>` / nav / `hreflang` blocks start breeding silent SEO
bugs: move to Zola (prebuilt binary, no `cargo install`).

## Multilingual

`/` is a `noindex` redirector reading `navigator.languages`. Real pages are
`/fr/`, `/en/`, `/es/`, with reciprocal `hreflang` and `x-default` on
`/en/`.

- The root is excluded by its `noindex`, **not** by `robots.txt`: a page
  blocked from crawling never delivers its `noindex`.
- Detection walks the list **in order** comparing the **primary subtag**.
  `navigator.language.startsWith('fr')` would miss `es-419` / `es-MX`, and
  `["de-DE", "fr"]` must lead to French, not the English fallback.
- Redirect with `location.replace()`. With `location.href` the root stays in
  history and Back loops forever.

## Indexing

Search Console: **Domain** property on `loulouw-labs.fr`, verified by a
root TXT next to the SPF (several TXT on one name are valid; only SPF must
be unique). Covers future subdomains without re-verification.

IndexNow (Bing, Yandex): the key **is** the name of `site/<key>.txt`, which
contains the key itself. Deploy the key file before pinging. Resubmit after
adding pages:

```powershell
$key = (Get-ChildItem site\*.txt -Exclude robots.txt).BaseName
Invoke-WebRequest -Uri https://api.indexnow.org/indexnow -Method Post `
  -ContentType 'application/json; charset=utf-8' -Body (@{
    host = 'rorganizer.loulouw-labs.fr'; key = $key
    keyLocation = "https://rorganizer.loulouw-labs.fr/$key.txt"
    urlList = @('https://rorganizer.loulouw-labs.fr/fr/')
  } | ConvertTo-Json)
```

`202 Accepted` = taken into account. `403` = key not validated, check the
file is served.

Site name: Google reads `WebSite` JSON-LD from the home page. Here that is
the `noindex` root, which sends Googlebot (no `Accept-Language`) to `/en/`,
so both carry the same `WebSite` block. Keep them identical.

## CSS

- Grids: `minmax(0, 1fr)`, never bare `1fr`. Grid children keep
  `min-width: auto`, so a `<pre class="term">` pushed `scrollWidth` to
  842 px in a 500 px window. `overflow-x: auto` on the `<pre>` is not
  enough.
- Prefer direct-child over descendant selectors for rules aimed at one
  element: class + element beats class alone. `.gate img` blew the flags up
  to 56×56 and `.features li span` turned nested `span.key` into full-width
  blocks; fixed as `.gate > img` and `.features li > span`.
- Diagnose cascade issues by measuring, not by reasoning on specificity:
  inject a script printing `getComputedStyle` into a copy of the page and
  render it headless.

## Colors

The app green `#97c459` on `#f4f5f0` is 1.85:1. Light theme: text accent
`#4f7020` (5.2:1, AA), `#67932e` (3.4:1) for text-free surfaces only. Dark
theme: `#97c459` as is (8.9:1). Light panels stand out by drop shadow; in
dark, shadows don't read, so relative luminance + border take over and the
shadow token is `none`.

`<meta name="theme-color">` carries `--ground` per `prefers-color-scheme`;
`app.js` overwrites both with the explicit theme when the toggle (or a saved
choice) wins. Change `--ground` → update the metas and `GROUND` in `app.js`.

## Fonts

Local subsetting needs `pyftsubset` or `glyphhanger`, i.e. Python or Node —
not installed. Take the `latin` `.woff2` served by the Google Fonts CSS API
(covers FR and ES, `¿` and `¡` included). Keep the `OFL-*.txt` next to
them: the OFL requires it. Literata is variable (one file covers 400→600,
hence `font-weight: 400 600`); Archivo 700 and JetBrains Mono 400 are
static, one weight each.

## Hero animation

A green marker steps down the account rows of the screenshot, in sync with
keys lighting up under the text: it demonstrates the product mechanism.

- Coordinates are **pixel-measured** on `app-*.png` — rows at
  y=167/257/347/437 of 968, x=24 to x=656 of 680 — then expressed in %. If
  the app UI changes, re-measure; don't adjust by eye.
- The marker moves by `transform: translateY()`, never `top`: a `top`
  change counts toward CLS. `translateY` percentages are relative to the
  marker's own height (84 px), so a 90 px row step is 107.143 %.
- **Finite** (three passes): a perpetual hero animation distracts.
- `steps(1, end)`: a key press is instant, a glide would misrepresent it.

## Regenerating app screenshots

Site captures are **680×968**, twice the README ones.

1. Display at **200 %**, launch the Dofus clients wanted — without them
   the app shows "0 compte lancé".
2. Make the capturing process DPI-aware before any graphics call
   (`SetProcessDpiAwarenessContext(-4)`).
3. `SetWindowPos(HWND_TOPMOST)` + `SetForegroundWindow` before copying.
4. Windows 11 corners are rounded but `GetWindowRect` is a rectangle: cut
   along a rounded rectangle (16 px radius at 200 %) and leave the corners
   transparent.

Stage shortcuts through `config.json` (four accounts + both cycle
directions): **back up the user's config and restore it byte-identical**.
Switch language with the `lang` field. Single instance: wait for the
process to fully exit between launches.

## Images

PNGs are compressed by hand outside the repo (787 → 417 KB). Lossy: `og-*`
cards go to 8-bit indexed, `icon.png` alpha changes. Visually equivalent at
display sizes, checked on the card at 640 px (Discord) and the icon at
56 px on dark.

`docs/gen-social-preview.ps1` (unversioned) reads
`site/assets/img/app-<lang>.png`, not `resources/screenshots/`: the share
card must show the hero's state. ⚠ It outputs **uncompressed** PNGs:
recompress after regenerating, or the site silently gains ~180 KB.

## Deliberately not on the site

- `perf-*.svg`: 12 px labels end up ~6.5 px on a phone. Metrics are rebuilt
  in HTML.
- Class icons: Ankama's property. Covered by the disclaimer inside the app,
  not as standalone ornament on a public promo site. They already appear in
  the app screenshot; the keyboard-key motif replaces them.
- Hardcoded SHA256: changes every release. Show commands; GitHub shows the
  hashes.

## Headless rendering (Brave / Chromium)

- Working combination here: `--headless=new --disable-gpu --no-sandbox
  --virtual-time-budget=3000`, no `--user-data-dir`. An open Brave instance
  can swallow the command; a fresh profile can hang.
- The output file is written **after** the process returns: check it in a
  separate call.
- Minimum window width is ~500 px and `--force-device-scale-factor` doesn't
  shrink the CSS viewport. For a real 390 px render, load the page in an
  `<iframe width="390">`.
- `--force-dark-mode` is Chromium's auto-inversion, not
  `prefers-color-scheme: dark`. Set `data-theme="dark"` on `<html>`, the
  theme toggle's real code path.
