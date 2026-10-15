# Diffract

[![Platform: macOS](https://img.shields.io/badge/platform-macOS-lightgrey)](https://github.com/nassah221/diffract/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)

**Diffract is a fork of [Diffity](https://github.com/nilbuild/diffity).** This fork adds Go navigation powered by `gopls`
inside the review diff. Diffity is MIT-licensed, © Kamran Ahmed; changes made in this fork are © 2026 nassah221.
[FORK.md](./FORK.md) records the differences, upstream sync instructions, and attribution terms.

Diffract is a Mac app for reviewing code changes, yours or your agent's, GitHub-style, with Claude Code in the loop.

<img src="./.github/screenshots/hero.png" alt="Diffract showing uncommitted changes with a comment thread answered by Claude" />

[![Download Diffract for macOS](https://img.shields.io/badge/Download_for_macOS-000000?style=for-the-badge&logo=apple&logoColor=white)](https://github.com/nassah221/diffract/releases)
[![Report an issue](https://img.shields.io/badge/Report_an_issue-ffffff?style=for-the-badge&logo=github&logoColor=black)](https://github.com/nassah221/diffract/issues/new/choose)

## Download and install

Download the macOS DMG from [Diffract Releases](https://github.com/nassah221/diffract/releases), open it, and move Diffract to Applications. The app is currently unsigned: on first launch, Control-click (or right-click) Diffract and choose **Open**, then confirm. Alternatively, after moving it to Applications, run:

```sh
xattr -dr com.apple.quarantine /Applications/Diffract.app
```

Updates are manual: install a newer DMG from Releases. Diffract has no in-app updater or **What's New** screen.

## Requirements

Diffract runs on macOS 13.3 or later; check each release for the architecture of its DMG. Claude Code (logged in) and Node.js are needed for AI features. Go navigation requires `gopls` to be installed and available to the app.

## Build from source

With Rust, Node.js, pnpm, and Xcode Command Line Tools installed, run `pnpm install` and then `pnpm tauri build` to build the app and DMG. The Liquid Glass icon catalog needs full Xcode to compile; without Xcode, the build ships with the rasterized `.icns` icon.

| What can you do? | Description |
|---|---|
| [Comment on your diff](#comment-on-your-diff) | Leave comments on lines, files or the whole diff, then send them to Claude to fix |
| [Let Claude fix your comments](#let-claude-fix-your-comments) | Claude works through your comments, edits the code and replies in each thread |
| [Ask @claude](#ask-claude) | Mention @claude in any thread and Claude replies right there |
| [Get a Claude review](#get-a-claude-review) | Claude reads the diff and leaves comments on the lines it cares about |
| [Review any commit or range](#review-any-commit-or-range) | Uncommitted changes, a commit, a range or a whole branch |
| [Review pull requests](#review-pull-requests) | Check out a GitHub PR, review it locally, post the review back |
| [Post reviews to GitHub](#post-reviews-to-github) | Submit as Comment, Approve or Request changes, with GitHub threads synced into the diff |
| [Browse and comment on any file](#browse-and-comment-on-any-file) | Comment on code outside the diff from the Files tab |
| [Jump anywhere](#jump-anywhere) | ⌘P for files, ⌘K for commits, comments, PRs and every action |
| [Switch projects](#switch-projects) | Keep your repos in a rail and switch with ⌘1–9 |
| [Open from the terminal or your agent](#open-from-the-terminal-or-your-agent) | `diffract` opens a repo, a diff or a pull request URL in the app |
| [Navigate Go code](#go-navigation) | Use gopls for hover documentation, inlay hints, definitions and implementations |

### Comment on your diff

> Drag across line numbers or click the `+` in the gutter, write a comment, then hit **Send to Claude** to have it fix them.

<img src="./.github/screenshots/comment.gif" alt="Selecting lines in a diff and leaving a comment" />

### Let Claude fix your comments

> Hit **Send N to Claude**; it works through each comment, edits the code and replies in the thread, and you review its changes like any other diff.

<img src="./.github/screenshots/claude-fix.gif" alt="Sending two comments to Claude and its edits and replies landing on the diff" />

### Ask @claude

> Type `@claude` in any comment or reply and Claude answers in the same thread.

<img src="./.github/screenshots/claude-reply.gif" alt="Mentioning @claude in a comment and Claude replying in the thread" />

### Get a Claude review

> Click **Ask Claude to review**, optionally add what to focus on, and its comments land on the diff as it works.

<img src="./.github/screenshots/claude-review.gif" alt="Asking Claude to review and its comments appearing on the diff" />

### Review any commit or range

> Open the picker next to the project name to switch between uncommitted changes, a commit, a range or a branch.

<img src="./.github/screenshots/refs.gif" alt="Searching commits in the ref picker and opening one" />

### Review pull requests

> Pick a PR from the branch switcher to check it out, comment as you read, then **Submit review** to post it to GitHub.

<img src="./.github/screenshots/pull-request.gif" alt="Checking out a pull request from the branch switcher and opening the review panel" />

### Post reviews to GitHub

> Comment as you read, then **Submit review** as Comment, Approve or Request changes; GitHub comments sync back into the diff so you can reply or resolve from the app.

<img src="./.github/screenshots/post-review.gif" alt="Drafting a review comment on a pull request next to synced GitHub threads and approving it" />

### Browse and comment on any file

> Open the **Files** tab, pick any file in the repo and comment on its lines, even code the diff doesn't touch.

<img src="./.github/screenshots/files.gif" alt="Opening a file from the Files tab and commenting on two of its lines" />

### Jump anywhere

> Press `⌘P` to open any file, or `⌘K` to search commits, comments, PRs and every action.

<img src="./.github/screenshots/palette.gif" alt="Opening a file with ⌘P and switching to dark theme with ⌘K" />

### Switch projects

> Open a repo with `⌘O`; it stays in the left rail, so `⌘1`–`⌘9` takes you back to it.

<img src="./.github/screenshots/projects.gif" alt="Switching between projects in the rail" />

### Open from the terminal or your agent

> Run `diffract` in any repository to open it in the app, or pass a pull request URL to review it.

Install the command once from the app menu: **Diffract → Install ‘diffract’ Command…**. It links the command into `/usr/local/bin` and asks for your password if that folder needs it.

<img src="./.github/screenshots/install-command.png" width="320" alt="The original Diffity menu showing its command-install option" />

```sh
diffract                                       # uncommitted changes in the current repository
diffract ~/code/app                            # another repository
diffract --ref main                            # everything on this branch since it left main
diffract --ref HEAD~1..HEAD                    # the last commit
diffract https://github.com/owner/repo/pull/42 # check out and review a pull request
diffract src/app.ts                            # a file in the Files tab
diffract -n                                    # in a new window
```

`--ref` takes anything the ref picker does: `work`, `staged`, `unstaged`, a branch or commit, `<a>..<b>` or `<a>...<b>`. A pull request opens in the current repository if it is a clone of that repo, else in a clone Diffract already knows; with neither, Diffract offers to clone it. Run `diffract --help` for everything.

Coding agents can run it too, so you can ask Claude Code to “open your changes in Diffract” before it opens a PR.

### Go navigation

Go navigation is Diffract's addition to Diffity. In Go (`.go`) files in the review diff, hover a symbol for its signature and documentation, use **⌘-click** to go to its definition or **⌘⌥-click** to go to an implementation, and see gopls inlay hints for parameter names and inferred types. The target opens in the app, with a brief highlight at the destination. This feature requires `gopls`; only Go files are supported.

### More

- **Find in the whole diff.** `⌘F` searches every file in the diff, including collapsed ones, and `⌘G` jumps to the next match.
- **Changed since you viewed it.** Files you marked Viewed get a badge when they change again, and one click shows just what changed.
- **Hide generated files.** Add a `.diffityignore` to your repo (same syntax as `.gitignore`), or a list in Settings, and those files stay out of your diffs:

  ```gitignore
  dist/
  *.generated.ts
  !dist/keep-me.js
  ```
- **Colour-blind friendly colours.** Settings → General → Appearance switches the diff to blue and orange, in light and dark.
- **Split or unified, light or dark.** Toggle the diff layout anytime; the theme follows your Mac unless you pick one in Settings.
- **App identity.** Bundle ID: `com.diffract.app`; deep links use `diffract://` (`diffract-dev://` in development). App data is stored in `~/Library/Application Support/com.diffract.app/diffract.db`.

Found a bug or have an idea? [Open an issue](https://github.com/nassah221/diffract/issues/new/choose).

## License

[MIT](./LICENSE) © [Kamran Ahmed](https://x.com/kamrify) · fork changes © 2026 nassah221
