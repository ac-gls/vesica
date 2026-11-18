# Personal vault: agent instructions

This is a private, per-person vault: the agents' working memory. `Team/` is the team's
shared home (a separate repository) and is read-only for agents.

## Read order
1. This file, then `README.md` (the vault index).
2. `Team/README.md`, then the relevant `Team/Knowledge/<area>/README.md`.
3. Only the notes the task needs.

## Write rules
- End every agent task with a ledger note in `Sessions/agent-runs/`
  (`vesica vault new-note --task <task> --agent <agent>`).
- Every note needs YAML frontmatter: `title, created, updated, type, area, tags`.
  Allowed `type`: standup-report, report, guide, checklist, readme, template, status, audit,
  workflow, troubleshooting, session-log, note, daily-note.
- Personal decisions go in `Decisions/`. Team decisions are drafted there and promoted by a
  person; agents never write into `Team/`.
- Never write secrets. Never write outside the paths the task authorises.

`vesica vault check` validates this contract; `vesica vault snapshot` commits the vault to its
own Git repository and never includes `Team/`.
