# Contributing to klon3d

Humans and agents follow the same rules. What applies to the content is in [AGENTS.md](AGENTS.md); this file
describes the **workflow** – how tasks are handed out so that several agents can work at the same time
without overwriting each other's files.

`main` on GitHub (`viktoschi/GarmentCraft`) is the shared state. Nobody pushes to `main` directly; every change
arrives as a pull request and is merged by viktoschi.

## In short

1. Find a free task: `tools/task.sh list`
2. Claim it: `tools/task.sh claim <nr> <session>` – checks for overlaps, sets `in-progress` and writes the
   claim into the issue
3. Your own worktree, branch `<nr>-<keyword>` from the current `origin/main`
4. A small, verifiable change with a test; only files from the task's scope
5. Pull request with `Closes #<nr>`, evidence and limits, then `tools/task.sh done <nr> <session> <pr>`
6. Stopping or stuck: write a handoff note, then `tools/task.sh release <nr> <session> "<note>"`

`task.sh` only needs `gh` (logged in) and runs on Linux, macOS and Git Bash. `tools/task.sh help` lists all
commands.

## Tasks

A task is an issue created from the **Task** template. It states an observable **goal**, a verifiable
**acceptance** and, under **Files**, its scope: one file or folder per line, relative to the repository
root, folders end with `/`, no wildcards like `*`.

| Label | Meaning |
|---|---|
| `task` | the issue is a task created from the template |
| `ready` | free, can be claimed |
| `in-progress` | claimed; who has it is in the claim comment |
| `review` | pull request open, waiting for review |
| `blocked` | waiting for another task or a decision |
| `needs-clo-data` | can only be checked on the main machine |
| `shared-file` | touches a shared file ([AGENTS.md](AGENTS.md)) |
| `bug` | bug report, not yet a task |

Anyone may create tasks, including an agent in the middle of its work that finds something outside its own
scope. Small is better: two tasks with separate files beat one that reserves half the workspace. Without the
web form:

```sh
gh issue create -R viktoschi/GarmentCraft --label task,ready --title "[Task] …" --body-file task.md
```

`task.md` needs the same sections as the template – `task.sh` reads the scope from `### Files`:

```markdown
### Goal
…
### Acceptance
…
### Files
crates/klon3d-sim/src/friction.rs
crates/klon3d-sim/tests/friction.rs
### Test data
runs without CLO data
```

## Claim before you change anything

`task.sh claim` compares the scope with every claimed task and with the files of every open pull request. If
paths overlap – the same file, or one inside the other's folder – it stops. Then pick another task or split
the scope in the issue.

All agents write with the same GitHub account. Who owns a task is therefore stated in the claim comment, not
in the assignee field: **the first `Claimed` comment since the last release wins.** If two agents claim at
the same time, the second one notices when it reads the comments back; `claim` reports it and the second one
lets go.

The session name is free but unique and recognisable: `<tool>-<keyword>-<MMDD>`, for example
`claude-seams-1031` or `codex-dxf-1102`.

A claim is an agreement between cooperating agents, not a file lock. If you need more files, add them to the
issue's scope and run `task.sh check <nr>` again **before** touching them.

## Work in your own worktree

Every task gets its own worktree – never two agents in the same directory.

```sh
git fetch origin
git worktree add ../klon3d-wt/12-seam-sliding -b 12-seam-sliding origin/main
```

On the main machine worktrees live under `D:\ai\klon3d-wt\` (see [AGENTS.md](AGENTS.md)). Cloud agents work
in their own clone on a branch with the same naming scheme. Claude Code sessions with the worktree option
create the worktree themselves; name the branch after the same pattern.

- Before opening the PR, catch up with `main`: `git fetch origin && git rebase origin/main`, as long as nobody
  builds on your branch.
- Do not merge other people's branches, do not reformat other people's files, do not revert other people's
  work.
- After the merge, remove worktree and branch: `git worktree remove ../klon3d-wt/<branch>`.

## The pull request

One PR per task. The template asks for:

- **Change** – before and after.
- **Evidence** – the commands that actually ran, with their results. Generated text is not evidence.
- **Not checked** – for example `*_clo.rs` skipped because CLO data is missing. The task then carries
  `needs-clo-data`, and the run on the main machine happens before the merge.
- **Limits and provenance** – what does not work, where a method comes from, which shared files are touched.

CI ([.github/workflows/ci.yml](.github/workflows/ci.yml)) builds and tests on Linux without `klon3d-view` and
builds `klon3d-view` on Windows. For the `.zprj` tests it fetches the 47 public CLO projects from
`tabjy/clo3d-patterns` (commit `22d0c8a`, via `KLON3D_FREMD`). It has no CLO installation, no private
reference projects and no private patterns; those tests report "skipped". Green therefore means: it builds,
the data-free tests pass and the `.zprj` reader holds on 47 third-party projects. What counts is the run for
the latest state of the PR – a cancelled older run proves nothing.

viktoschi merges, preferably as a squash. Agents do not merge, not even their own PR.

## Handoff

Whoever stops – finished or not – leaves a note on the issue that lets the next session continue without
your chat history:

```text
Agent: <session>
Task: #<nr>
Base: <commit of origin/main>
Branch / PR: <branch>, #<pr>
Result: <observed behaviour or finding>
Evidence: <commands that actually ran and their output>
Not checked: <what is missing and why>
Blocked by: <question or #nr>
Next: <what remains>
```

Research notes separate **hypothesis**, **measurement** and **rejected** (with the reason). A rejected
approach is a result and saves the next session hours.

If a task sits in `in-progress` for three days without a new comment, viktoschi decides. Tasks are never
taken over silently.

## A prompt for a new agent

> Read AGENTS.md and CONTRIBUTING.md in viktoschi/GarmentCraft. Use `tools/task.sh list` to find a free task that
> fits this session and claim it with `tools/task.sh claim <nr> <session>`. You may post claim, progress and
> handoff comments on that issue. Work in your own worktree on the branch `<nr>-<keyword>`, change only files
> from the scope and open a pull request listing the checks that actually ran. Ask me before widening the
> scope. Do not merge. If you get stuck, leave a handoff note and release the task.

## Helping without code

A bug report (template **Bug**) with the pattern or project, the command, the expected and the observed
result and the commit is a full contribution. Attach only what may be shared – no third-party patterns, no
body measurements of other people, no CLO files.
