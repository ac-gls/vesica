# Security policy

`vesica` edits dotfiles and agent configuration on developer machines, so we treat bugs that cause it to
overwrite files, leak secrets, or run unexpected commands as security issues.

- Report privately via GitHub Security Advisories on this repository ("Report a vulnerability").
- Do not open a public issue for a vulnerability.
- The kit never reads, prompts for or prints secret values; auth is checked by status and expiry only.
- Release binaries are published with `SHA256SUMS`; `install.sh` and `vesica update` refuse a binary whose
  checksum does not match.
