# Vesica

One installer that gives every developer on a team the same agent setup on **Windows + WSL Ubuntu**:
team rules, shared skills, role agents, coding standards, repo pointers and model configuration for
**Claude Code, OpenCode, Copilot CLI, Codex** and **Hermes**, plus a personal Obsidian vault that the agents
use as shared memory.

`vesica` is content-free: it holds no team content. Your team's rules, skills and knowledge live in a
separate (usually private) **shared home** repository that the installer clones into your vault at `Team/`.

```bash
curl -fsSL https://raw.githubusercontent.com/ac-gls/agent-home/v0.1.0/install.sh \
  | sh -s -- --home-repo <owner>/<shared-home> --profile developer
vesica doctor
```

Nothing is overwritten: supported installer changes are limited to managed imports and symlinks recorded in
`~/.config/vesica/manifest.json`. Uninstall verifies recorded content and preserves entries it cannot
safely verify. This remains a prototype; see the onboarding and architecture notes before trying it.

See [docs/ONBOARDING.md](docs/ONBOARDING.md) for the current safe local trial procedure and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for implemented behavior and scope gaps. Do not use this prototype on a real user profile yet.

## License

MIT
