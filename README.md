# Vesica

One installer that gives every developer on a team the same agent setup on **Windows + WSL Ubuntu**:
team rules, shared skills, role agents, coding standards, repo pointers and model configuration for
**Claude Code, OpenCode, Copilot CLI, Codex** and **Hermes**, plus a personal Obsidian vault that the agents
use as shared memory.

`vesica` is content-free: it holds no team content. Your team's rules, skills and knowledge live in a
separate (usually private) **shared home** repository that the installer clones into your vault at `Team/`.

```bash
curl -fsSL https://raw.githubusercontent.com/ac-gls/vesica/v0.1.0/install.sh \
  | sh -s -- --home-repo <owner>/<shared-home> --profile developer
vesica doctor
```

# Team checkout Git status and pull

```bash
vesica team status --path /path/to/shared-home
vesica team pull --path /path/to/shared-home
```

`status` reads local refs only and never fetches. `pull` fetches the configured upstream and fast-forwards only a clean checkout; it refuses uncommitted changes and non-fast-forward histories. The checkout path can also be provided with `VESICA_TEAM_HOME`. This does not yet implement shared-home bootstrap, `vesica share`, branch/PR creation, or the complete human-mediated contribution workflow.

Finding output includes relative paths, line numbers and rule IDs; it deliberately omits document contents and matched credential text. The linter is heuristic and does not establish production readiness.

Nothing is overwritten: supported installer changes are limited to managed imports and symlinks recorded in
`~/.config/agent-home/manifest.json`. Uninstall verifies recorded content and preserves entries it cannot
safely verify. This remains a prototype; see the onboarding and architecture notes before trying it.

See [docs/ONBOARDING.md](docs/ONBOARDING.md) for the current safe local trial procedure and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for implemented behavior and scope gaps. Do not use this prototype on a real user profile yet.

The bootstrap can be exercised locally with `bash tests/install.sh`. It builds a fixture release tarball and checks bootstrap-only installation, safe argument forwarding, and checksum rejection without leaving an installed binary behind. CI runs this suite alongside ShellCheck; it does not download or publish a real release.

## License

MIT
