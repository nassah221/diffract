# Diffract

Diffract is a **fork of [Diffity](https://github.com/nilbuild/diffity)** — a Mac app for reviewing code changes, with
a coding agent in the loop — extended here with Go navigation (gopls) inside the diff: hover for a signature, ⌘-click
to follow a definition or an implementation, and jump straight to the file and line.

Upstream: <https://github.com/nilbuild/diffity>

## Licence and attribution

Diffity is MIT licensed, © 2026 Kamran Ahmed. The original [`LICENSE`](./LICENSE) is kept verbatim and its copyright
notice continues to cover this fork and every copy of it. Changes added by this fork are © 2026 nassah221, as recorded
in `git log`.

The app says the same thing in Settings → About, so a distributed build carries the attribution with it.

## Syncing with upstream

This fork is kept as a small stack of commits on top of `upstream/main`, so syncing is a **rebase**, not a merge:

```sh
git remote add upstream https://github.com/nilbuild/diffity   # once
git fetch upstream
git rebase upstream/main
```

Then re-run the checks before pushing:

```sh
pnpm typecheck && pnpm test
cd src-tauri && cargo test --lib
```

Rules that keep rebases cheap:

- Prefer adding a file over editing upstream's. When a change has to edit an upstream file, keep the hunk as small as
  it can be: no reformatting, no reordering, no drive-by edits.
- One concern per commit, message style `scope: lowercase summary` (`lsp:`, `build:`, `ui:`).
- This file is the contract for what has deliberately diverged. Add to it in the same commit that diverges further.

## Identity

| | Upstream | This fork |
| --- | --- | --- |
| product name / window title | Diffity | Diffract |
| bundle identifier | `com.diffity.app` | `com.diffract.app` |
| dev-channel identifier | `com.diffity.app.dev` | `com.diffract.app.dev` |
| app binary | `diffity-desktop` | `diffract` |
| deep-link scheme | `diffity://` | `diffract://`, `diffract-dev://` |
| installed CLI command | `diffity` | `diffract` |
| local data | `…/com.diffity.app/diffity.db` | `…/com.diffract.app/diffract.db` |

## Deliberately not renamed

Renaming these would break files, configs or integrations that already exist, for no user-visible gain — so they keep
upstream's spelling on purpose:

- `.diffityignore` at a repo root and `<git dir>/info/diffityignore`
- webview storage keys (`diffity-theme`, `diffity-diff-palette`)
- bundled sidecar binaries `diffity-cli` and `diffity-mcp` (Tauri `externalBin`), and the `DIFFITY_CHANNEL` build env var
- Rust crate and lib names (`diffity-desktop`, `diffity_desktop_lib`) — internal, and renaming them churns files upstream edits
- upstream's `web/` site directory, `src-tauri/icons/`, and the GitHub workflow files

## Divergences from upstream

- **gopls navigation** (`src-tauri/src/lsp/**`, `src-tauri/src/commands/lsp.rs`, `src/features/lsp/**`): hover card,
  go-to-definition/implementation, inlay hints, the eligibility gate, and the Tree-side landing flash plus the ⌘-hover
  pointer cursor.
- **Browser dev mock** (`src/lib/mock-api.ts`, `mock-api-agents.ts`, `mock-fixtures.ts`): runs the real frontend with
  canned IPC when there is no Tauri host, and `?mockDiff=<url>` can serve a recorded diff.
- **New icon artwork**: a beam entering a prism and leaving as three rays, with the colour fringing a lens would
  leave. It lives in `src/components/icons/brand-logo.tsx` (the in-app mark) and in
  `src-tauri/icons/diffract.icon/` plus `src-tauri/icons/dev/diffract-dev.icon/` (the Icon Composer documents:
  three offset layers, fringes behind the base, so the effect survives the mask-and-solid-fill model). The `.icns`
  and PNG ladder are rasterised from the same artwork. The Liquid Glass `Assets.car` is *not* committed: compiling
  an Icon Composer document needs full Xcode (`actool`), which this machine does not have.
- **The upstream update check is gone**: the launch/recheck hooks and their toast, the app-menu item, the Settings
  control, the palette commands, the release-notes fetch, and the updater plugin with its capability, endpoint, pubkey
  and artifact signing. It pointed at upstream's release feed, so an installed build would have offered to install
  Diffity over itself. A fork-owned update path is a follow-up, not an oversight.

## Known follow-ups

- "Report an Issue…" still opens upstream's tracker (`src/lib/report-issue.ts`, `src-tauri/src/commands/feedback.rs`).
- `.github/workflows/release.yml` and `dev-build.yml` are upstream's and still expect updater artifacts and signing secrets.
- `src-tauri/icons/**/*.icon` are sources only: on a Mac with Xcode, recompile them into `Assets.car` for the
  Liquid Glass icon, otherwise the rasterised `.icns` is what ships and shows.
- The boot splash in `index.html` still draws upstream's mark (it flashes for a few hundred ms), and Windows'
  `icon.ico` was dropped along with the upstream artwork since this fork has no Windows target.
