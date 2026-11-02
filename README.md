# repo-manager-tui

A terminal UI for finding the code repositories on your disk, seeing what they
cost in storage, and reclaiming that space — by clearing regenerable junk or by
deleting repos that are already safely on a remote.

Built with [Ratatui](https://ratatui.rs).

## Features

- **Discovery** — finds projects under one or more folders by `.git` or common
  manifest files (`package.json`, `Cargo.toml`, `go.mod`, `pyproject.toml`, …).
- **Disk overview** — free and used space on the volume, total repo size, and
  each repo's share of the disk.
- **Junk detection** — dependencies (`node_modules`, `.venv`, `Pods`) and
  caches/build output (`target`, `dist`, `.next`, `.turbo`, `__pycache__`, …),
  matched by name *and* sibling files so source folders are never misidentified.
- **Git safety check** — every repo is classified:

  | Status        | Meaning                                         |
  | ------------- | ----------------------------------------------- |
  | `SAFE`        | has a remote, clean tree, nothing unpushed, no stashes |
  | `STASHED`     | stashes exist that would be lost               |
  | `UNPUSHED`    | commits not present on any remote              |
  | `UNCOMMITTED` | modified or untracked files                    |
  | `NO REMOTE`   | git repo without a remote                      |
  | `NO GIT`      | not a git repo at all                          |

- **Claude history** — Claude Code's chat history (`~/.claude/projects`) is
  matched to the folder each conversation was started in. History for parent
  folders (e.g. a workspace holding several repos) appears as a `FOLDER` row,
  and history for folders that no longer exist appears as a `GONE` row.
  Claude's `memory/` and any session active in the last 15 minutes are kept.
- **Clean** dependencies, caches, Claude history, or all of them — for one repo or many.
- **Delete** whole repos, with stronger confirmation for anything not backed up.
- Search, filters, sorting, and multi-select.

## Install

Requires Rust 1.88+ and `git` on your `PATH`. Supports macOS and Linux.

```sh
cargo install --path .
```

## Usage

```sh
repo-manager ~/code              # scan one folder
repo-manager ~/code ~/work       # scan several
repo-manager --depth 6 ~/code    # search deeper (default: 4)
```

### Keys

| Key            | Action                                        |
| -------------- | --------------------------------------------- |
| `↑ ↓` / `j k`  | move; prefix a count to repeat (`10j`, `10k`)  |
| `gg` / `G`     | first / last row; `5G` jumps to row 5          |
| `space`        | mark / unmark                                 |
| `a`            | mark / unmark all visible                     |
| `n` `c` `h` `x`| clean dependencies / caches / Claude history / all |
| `d`            | delete projects                               |
| `o`            | open in file manager                          |
| `/`            | search by name                                |
| `f`            | filter: all → safe → at risk → has junk       |
| `s`            | sort: size → junk → status → last commit → name |
| `S`            | reverse sort direction                        |
| `r`            | rescan                                        |
| `Esc`          | clear search, then marks                      |
| `?`            | help                                          |
| `q`            | quit                                          |

Actions apply to all marked projects that are currently visible, or to the
highlighted project when nothing is marked.

## Safety

Deleting is permanent — nothing goes to the Trash. The tool is designed so that
losing work requires deliberate intent:

- Every destructive action shows exactly what will be removed and asks first.
- Deleting a project that is not `SAFE` requires typing `delete`.
- `SAFE` projects are re-checked immediately before deletion and skipped if
  anything changed since the scan.
- Cleaning only removes recognised junk folders inside the project, and Claude
  history only inside the Claude projects folder.
- `FOLDER` and `GONE` rows can only have their Claude history cleaned, never be deleted.
- Symlinks are never followed; scan roots, their parents, paths outside the
  roots, and the current working directory are never deleted.

The unpushed check compares against your local remote-tracking refs; it does
not fetch. Run `git fetch` first if a remote may have been rewritten.

Set `CLAUDE_CONFIG_DIR` if your Claude Code config lives somewhere other than `~/.claude`.

## Development

```sh
cargo run -- ~/code
cargo test
cargo clippy --all-targets
```

`src/` layout:

| Module        | Responsibility                                   |
| ------------- | ------------------------------------------------ |
| `discover`    | find project folders                             |
| `measure`     | disk usage and junk detection                    |
| `git`         | repository status and safety classification      |
| `clean`       | remove junk folders                              |
| `claude`      | locate, attribute, and clear Claude history      |
| `delete`      | delete projects with path guards                 |
| `disk`        | volume capacity via `statvfs`                    |
| `app/`        | state, key handling, background tasks            |
| `ui/`         | rendering                                        |
