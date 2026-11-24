# Security policy

`vesica` edits dotfiles and agent configuration on developer machines, so we treat bugs that cause it to
overwrite files, leak secrets, or run unexpected commands as security issues.

- Report privately via GitHub Security Advisories on this repository ("Report a vulnerability").
- Do not open a public issue for a vulnerability.
- Auth checks use status/expiry, not secret prompts. Credential scanners necessarily inspect file bytes but report only paths, lines and rule IDs, never matched tokens. A clean heuristic scan is not proof of secret absence; known image/PDF assets are explicitly reported unscanned exemptions, including snapshot/share mutation gates.
- Release binaries are published with `SHA256SUMS`; `install.sh` and `vesica update` refuse a binary whose
  checksum does not match. Staged identity must also match the requested tag with a successful version command. Checksums fetched beside an artifact establish integrity against that checksum, not authenticated publisher provenance; signatures are not implemented.
- Filesystem rollback is guarded and best-effort for recoverable returns. Intervening edits, node replacements and cleanup errors are retained/reported. No arbitrary crash/power-loss, directory-handle race-proofing or rollback of arbitrary candidate-process writes is claimed. Use disposable roots until review and native release evidence cover the intended version.
