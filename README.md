# agent-home

One installer that gives every developer on a team the same agent setup on **Windows + WSL Ubuntu**:
team rules, shared skills, role agents, coding standards, repo pointers and model configuration for
**Claude Code, OpenCode, Copilot CLI, Codex** and **Hermes**, plus a personal Obsidian vault that the agents
use as shared memory.

`agent-home` is content-free: it holds no team content. Your team's rules, skills and knowledge live in a
separate (usually private) **shared home** repository that the installer clones into your vault at `Team/`.

```bash
curl -fsSL https://raw.githubusercontent.com/ac-gls/agent-home/v0.1.0/install.sh \
  | sh -s -- --home-repo <owner>/<shared-home> --profile developer
agent-home doctor
```

Nothing is overwritten: every change is a symlink, a marked block (`# >>> agent-home >>>`) or a merged
entry, recorded in `~/.config/agent-home/manifest.json`, and `agent-home uninstall` removes it exactly.

See [docs/ONBOARDING.md](docs/ONBOARDING.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md).

## License

MIT
