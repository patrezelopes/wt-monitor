---
name: agent-orchestration
description: Coordinate several sub-agents on a backlog — implementers in parallel (one Git worktree per task), reviewers per pull request, and QA in sequence. Use when starting or continuing a batch of independent tasks. The coordinator never edits application code.
---

# Agent orchestration

## Role

Coordinate sub-agents. **Do not edit application code.** Your outputs are dispatches, a state table and handoffs.

## Parallelism

- **Implementers:** `PARALLEL=2` by default. Launch several `Agent` calls in the **same message** only if the tasks are independent (different repos, or domains that do not touch the same files).
- **Reviewers:** one per open pull request.
- **QA:** sequential (`QA_PARALLEL=1`). A shared test environment is a shared resource.

## One worktree per task

Each implementer works in its own Git worktree on its own branch, so parallel agents never fight over a working directory:

```bash
git worktree add ../wt-<task-id> -b feature/<task-id> origin/main
```

When the task is merged, remove the worktree. A dashboard such as wt-monitor shows each worktree's CPU, memory and agents while they run.

## Claims (prevent two agents on one task)

Before launching an agent:

1. Read `docs/AGENT-CLAIMS.md`. If the task ID already has an active claim, stop.
2. Otherwise add a row: `slot`, `task id`, `branch`, `agent`, `timestamp`.
3. Mark the card as claimed in your tracker (through the board owner, see the `agile-coach` agent).
4. Only then launch the agent.
5. When the agent finishes (implementation or review), mark the claim `resolved`.

## Sub-agent roles

| Role | Instructions | Output |
|---|---|---|
| Implementer | `docs/prompts/IMPLEMENTER.md` | branch + pull request + test evidence |
| Reviewer | `docs/prompts/REVIEWER.md` | approve and merge, or send back with reasons |
| QA | `docs/prompts/QA.md` | verdict with evidence from the staging environment |

Keep these prompts in the repository, next to the code, so every agent reads the same rules.

## Evidence before "done"

Require in every pull request, from implementer and reviewer alike:

- the project's test command (for instance `make test`), run the way CI runs it
- a smoke check of the whole stack

A pull request without that evidence goes back to the implementer. Never accept "it should work".

## State table

Keep a single table and update it after every step:

| Task | Claim | PR | Review | Merged | QA |
|---|---|---|---|---|---|
| T-01 | slot 1 | #12 | approved | yes | pending |

| Moment | You write | Others read |
|---|---|---|
| Round start | slots + `dispatching` | the backlog |
| Before each launch | claim | the other coordinators |
| After an implementer | `reviewing` + PR link | the reviewers |
| After a reviewer | `awaiting_qa` | QA |
| After QA | `idle` + evidence | everyone |

## Context hygiene

When the conversation gets long, start a new one and paste only the state table plus the continuation prompt. Do not carry old transcripts forward.

## Anti-patterns

- Editing code "just this once". The moment the coordinator edits code, nobody is coordinating.
- Launching two agents on files that overlap, because the tasks "look" independent.
- Declaring success on a job that is `queued` or `in_progress`. Success is a completed run with a green conclusion.
- Letting a reviewer approve without test evidence.
