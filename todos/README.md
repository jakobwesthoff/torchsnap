# Todos

Each file is one piece of deferred work: a bug, a feature, a refactor,
an investigation or an open question. The file is all that carries
forward once the conversation that produced it is gone, so it has to
stand on its own.

## Layout

Todos are grouped by scope: the subsystem you would open the editor in
to work on it. Kind, severity and status live in the frontmatter, not
in the directory tree.

- `gadgets/<id>/`: work on a single gadget, WASM or built-in.
  `gadgets/future/` holds ideas for gadgets that do not exist yet.
- `gadget-host/`: the gadget runtime. `api/` for host APIs, `wasm/` for
  the WASM runtime, `caps/` for capability implementations, `install/`
  for install and uninstall, `sdk/` for the SDKs, `memory/` for
  footprint tuning, `architecture/` for design decisions, `audit/` for
  audits.
- `frontend/`: the React side. `devtools/`, `infra/`, `perf/`,
  `settings/`, `ux/`, `verticals/`.
- `backend/`: Rust and Tauri host code outside the gadget runtime.
  `concurrency/`, `control/`, `errors/`, `ipc/`, `metadata/`,
  `search/`, `settings/`, `storage/`.
- `platform/`: OS-specific bugs and integrations, with `linux/` and
  `windows/`.
- `product/`: user-facing features (`features/`) and visual identity
  (`visual-identity/`) that span frontend and backend.
- `build/`: build pipeline, CI and developer tooling.
- `chores/`: small cleanups with no better home.
- `plans/`: documents that coordinate several todos.

Put a todo in the most specific matching directory. Add a new
subdirectory once three or more related todos accumulate; until then
keep them at the parent level.

## File names

`<ulid>-<short-description>.md`, with the ULID from `mkulid -l`. The
ULID is the todo's stable identifier. Keep it when moving or renaming
the file.

## Frontmatter

Every todo starts with YAML frontmatter, then a blank line, then the H1
title. Fields appear in this order. Lists are always flow style
(`[a, b]`) so each field stays on one grep-able line. Leave out a field
that does not apply instead of writing it empty.

```yaml
---
kind: bug
severity: high
status: open
area: [src-tauri/src/caps/command.rs, src-tauri/src/wasm/argv_matcher.rs]
tags: [security, unconfirmed]
plan: todos/plans/<ulid>-<name>.md
depends-on: [todos/<scope>/<ulid>-<name>.md]
---
```

- `kind` (required): `bug`, `feature` (new capability), `improvement`
  (makes existing behavior better), `refactor`, `chore`,
  `investigation`, `question`, `decision`, `docs`, or `plan` (only in
  `plans/`).
- `severity` (optional): `critical`, `high`, `medium`, `low`.
- `status` (required): `open`, `needs-discussion`, `blocked` (waiting on
  a named todo or external event), `deferred` (parked on purpose, no
  named trigger), `in-progress`. Put any nuance in the first paragraph
  after the title.
- `area` (optional): repo-relative paths the todo is about.
- `tags` (optional): only from the list below. Do not tag what the
  directory already says (no `linux` under `platform/linux/`).
- `plan` (optional): the coordinating file in `plans/`.
- `depends-on` (optional): todos that must land first. Looser relations
  stay as prose in the body.

Tags:

- Platform: `macos`, `linux`, `windows`.
- Security and trust: `security` (vulnerability, hardening, trust
  boundary), `privacy` (data exposure or disclosure), `unconfirmed` (a
  suspected bug not yet reproduced).
- Correctness and quality: `concurrency`, `error-handling`,
  `performance`, `memory`, `testing`.
- Product: `ux`, `accessibility`, `docs`.
- Infrastructure: `dependencies`, `ci`, `tooling`, `migration` (large
  structural moves such as a framework major version), `api-design`,
  `logging`, `config`.
- Subsystems, when the directory does not already say it: `wasm`,
  `sdk`.

Add a tag to this list before using it anywhere.

## Writing a todo

After the title, capture the topic, what was found, any discussion and
the decisions made. Most bug todos use the sections `Problem`, `Impact`
and `Suggested fix`. Point at code with repo-relative `path:line`
references and link other todos by their path under `todos/`.

## Finding work

The frontmatter is built for `rg`:

```sh
rg -l '^severity: (critical|high)' todos/
rg -l '^status: open' todos/gadget-host/
rg -l '^tags: .*security' todos/
```

Line numbers in older todos drift as the code changes. Find the code
again before acting on a reference.

## Working on a todo

Bug fixes are test-first: a regression test that fails, then the fix
(see `CLAUDE.md`, "Tests and quality gates").

## Closing a todo

Once a todo is implemented, delete its file and every reference to it.
References live in other todos, in docs and ADRs, in code comments and
in the sibling repos `../torchsnap-docs` and `../torchsnap-web`, some
citing the full path and some only the file name.
`rg <ulid> . ../torchsnap-docs ../torchsnap-web` from the repository
root finds all of them. If a referencing file needs information from
the todo, copy just that part into it, compact and precise.

## Sweeping for finished work

After a large work package is done, Sonnet or Haiku agents check every
todo for work that has already been done. Todos that are fully done are
deleted as described above. Todos that are partly done are rewritten to
cover only the remaining work.
