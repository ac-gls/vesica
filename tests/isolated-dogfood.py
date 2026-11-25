#!/usr/bin/env python3
"""Credential-free native dogfood; fixed product source, disposable unprivileged user.
This is a test harness, not production implementation or published-release acceptance.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import stat
import subprocess
import sys
import time

BASE = Path('/opt/vesica-dogfood')
SOURCE = BASE / 'product'
HOME = Path.home()
E = BASE / 'evidence'
E.mkdir(exist_ok=True)
EXPECTED = 'd6c1cdb296ceab7e2445a4517bd09f512da75911'
TAG = 'v0.1.0-alpha.4'
BIN = HOME / '.local/bin/vesica'
SHARED = HOME / 'source/team-home'
VAULT = HOME / 'Documents/Personal Vault'
REPOS = HOME / 'source/repos'
rows = []
commands = []


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def snapshot(p):
    result = {}
    if not p.exists() and not p.is_symlink():
        return result
    def visit(path):
        s = path.lstat()
        key = str(path.relative_to(p))
        node = {'mode': stat.S_IMODE(s.st_mode), 'uid': s.st_uid, 'gid': s.st_gid}
        if stat.S_ISLNK(s.st_mode):
            node.update(kind='symlink', target=os.readlink(path))
        elif stat.S_ISREG(s.st_mode):
            node.update(kind='file', size=s.st_size, sha256=digest(path))
        elif stat.S_ISDIR(s.st_mode):
            node.update(kind='directory')
        else:
            node.update(kind='other')
        result[key] = node
        if node['kind'] == 'directory':
            for child in sorted(path.iterdir()):
                visit(child)
    visit(p)
    return result


def snap(label, p=HOME):
    data = snapshot(p)
    (E / (label + '.json')).write_text(json.dumps(data, indent=2))
    return data


def check(name, truth, detail=''):
    rows.append({'name': name, 'status': 'PASS' if truth else 'FAIL', 'detail': detail})
    save()
    print(rows[-1], flush=True)
    if not truth:
        raise AssertionError(name + ': ' + detail)


def product_check(name, truth, detail=''):
    rows.append({'name': name, 'status': 'PASS' if truth else 'FAIL', 'category': 'product', 'detail': detail})
    save()
    print(rows[-1], flush=True)


def observe(name, truth, detail=''):
    rows.append({'name': name, 'status': 'VERIFIED' if truth else 'UNVERIFIED', 'detail': detail})
    save()
    print(rows[-1], flush=True)


def save():
    (E / 'results.json').write_text(json.dumps({'product_commit': EXPECTED,
        'pid': os.getpid(), 'uid': os.getuid(), 'home': str(HOME),
        'checks': rows, 'commands': commands}, indent=2))


def run(label, args, cwd=HOME, expected=0, env=None, timeout=120):
    assert re.fullmatch('[a-z0-9-]+', label)
    e = os.environ.copy()
    if env:
        e.update(env)
    started = time.monotonic()
    out = subprocess.run([str(a) for a in args], cwd=cwd, env=e,
                         stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                         stderr=subprocess.STDOUT, timeout=timeout)
    log = E / (label + '.log')
    log.write_bytes(out.stdout)
    record = {'label': label, 'argv': [str(a) for a in args], 'cwd': str(cwd),
              'exit': out.returncode, 'seconds': round(time.monotonic()-started, 3),
              'log': log.name, 'sha256': digest(log)}
    executable = shutil.which(str(args[0])) if not Path(args[0]).is_absolute() else str(args[0])
    if executable and Path(executable).is_file():
        record['executable'] = executable
        record['executable_sha256'] = digest(Path(executable))
    commands.append(record)
    save()
    print(label, 'exit', out.returncode, flush=True)
    text = out.stdout.decode(errors='replace')
    if expected is not None:
        check(label + ' exit', out.returncode == expected, text[-1400:] if out.returncode != expected else '')
    return text, out.returncode


def git(label, args, cwd):
    return run(label, ['git', '-c', 'maintenance.auto=false', '-c', 'gc.auto=0', *args], cwd=cwd)[0].strip()


def put(p, data, mode=None):
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_bytes(data if isinstance(data, bytes) else data.encode())
    if mode is not None:
        p.chmod(mode)


def main():
    kernel = run('kernel', ['uname', '-sr'])[0]
    container = subprocess.run(['systemd-detect-virt', '--container'], capture_output=True)
    check('native Linux host guard', 'microsoft' not in kernel.lower() and not Path('/.dockerenv').exists() and container.returncode != 0)
    check('unprivileged separate account', os.getuid() != 0 and HOME == Path('/home/vesica-dogfood') and os.getgroups() == [os.getgid()], str(os.getgroups()))
    check('credential environment absent', not [k for k in os.environ if re.search('TOKEN|SECRET|PASSWORD|API_KEY|AUTH|ADO_|AZURE_|AWS_', k)])
    check('no inherited home credentials', all(not (HOME / p).exists() for p in ['.ssh', '.aws', '.azure', '.git-credentials', '.config/gh', '.claude/.credentials.json', '.codex/auth.json']))
    run('host', ['bash', '-c', '. /etc/os-release; printf "%s\n" "$PRETTY_NAME"; id; systemd-detect-virt; df -T "$HOME"; mount | sort'])
    source_head = git('source-head', ['rev-parse', 'HEAD'], SOURCE)
    check('exact fixed source commit', source_head == EXPECTED, source_head)
    source_before = snap('source-before', SOURCE)
    seed_originals = {
        '.claude/CLAUDE.md': b'# Personal Claude rules\nPreserve these owner rules.\n',
        '.codex/AGENTS.md': b'# Personal Codex rules\r\nPreserve CRLF and no final newline',
        '.bashrc': b'# isolated owner shell\nexport OWNER_SENTINEL=preserved\n',
        '.config/opencode/opencode.json': b'{\n  // isolated personal settings\n  "theme": "tokyonight",\n  "instructions": ["~/owner-rules.md"],\n}\n',
        'owner-rules.md': b'# Personal rules\nOwner rule sentinel.\n',
        'notes/private.md': b'Private synthetic note. Never promote automatically.\n',
    }
    for p, data in seed_originals.items():
        put(HOME / p, data, 0o640 if p.endswith('.md') else 0o600)
    put(HOME / 'owner-dir/sentinel.txt', 'unrelated internal link referent\n')
    (HOME / 'owner-link').symlink_to(HOME / 'owner-dir', target_is_directory=True)
    git('git-identity', ['config', '--global', 'user.name', 'Isolated Tester'], HOME)
    git('git-email', ['config', '--global', 'user.email', 'tester@example.invalid'], HOME)
    git('git-maintenance', ['config', '--global', 'maintenance.auto', 'false'], HOME)
    git('git-gc', ['config', '--global', 'gc.auto', '0'], HOME)
    remote = HOME / 'source/sample-app.git'
    remote.parent.mkdir(parents=True, exist_ok=True)
    git('remote-init', ['init', '--bare', str(remote)], HOME)
    seed = HOME / 'source/sample-seed'
    seed.mkdir()
    put(seed / 'README.md', '# Disposable sample project\n')
    git('sample-init', ['init', '-b', 'main'], seed)
    git('sample-stage', ['add', 'README.md'], seed)
    git('sample-commit', ['commit', '-m', 'Disposable project baseline'], seed)
    git('sample-push', ['push', str(remote), 'HEAD:main'], seed)
    git('sample-head', ['symbolic-ref', 'HEAD', 'refs/heads/main'], remote)
    shutil.copytree(SOURCE / 'tests/fixtures/shared-home', SHARED)
    config = (SHARED / 'agent-home.toml').read_text()
    config = config.replace('https://github.com/example-org/sample-app.git', str(remote))
    put(SHARED / 'agent-home.toml', config)
    git('shared-init', ['init', '-b', 'main'], SHARED)
    git('shared-stage', ['add', '.'], SHARED)
    git('shared-commit', ['commit', '-m', 'Neutral isolated shared-home baseline'], SHARED)
    put(SHARED / 'AGENTS.md', (SHARED / 'AGENTS.md').read_bytes() + b'\nSHARED-RULE-DOGFOOD-MARKER\n')
    git('shared-marker-stage', ['add', 'AGENTS.md'], SHARED)
    git('shared-marker-commit', ['commit', '-m', 'Neutral rule marker'], SHARED)
    shared_before = snap('shared-initial', SHARED)
    artifact = BASE / 'dist' / ('vesica-' + TAG + '-x86_64-linux.tar.gz')
    run('artifact-checksum', ['sha256sum', '-c', 'SHA256SUMS'], cwd=artifact.parent)
    run('bootstrap', ['sh', SOURCE / 'install.sh', '--from-file', artifact, '--bootstrap-only'])
    check('bootstrap exact executable', digest(BIN) == digest(SOURCE / 'target/x86_64-unknown-linux-musl/release/vesica'))
    run('version', [BIN, '--version'])
    run('help', [BIN, '--help'])
    run('lint-neutral-home', [BIN, 'lint', '--home-repo', SHARED])
    install = [BIN, 'install', '--home-repo', SHARED, '--profile', 'developer', '--vault', VAULT, '--repos-dir', REPOS]
    before = snap('before-install-dry-run')
    run('install-preview', install + ['--dry-run'])
    after = snap('after-install-dry-run')
    check('install preview zero mutation', before == after)
    run('install', install)
    check('repository actually cloned', (REPOS / 'sample-app/.git').is_dir())
    check('vault Team points only to disposable home', (VAULT / 'Team').is_symlink() and (VAULT / 'Team').resolve() == SHARED)
    backup_paths = [Path(x['backup']) for x in json.loads((HOME / '.config/agent-home/manifest.json').read_text())['entries'].values() if x.get('backup')]
    check('owner files backed up privately', len(backup_paths) >= 4 and all(p.is_relative_to(HOME) and p.is_file() and stat.S_IMODE(p.stat().st_mode) == 0o600 for p in backup_paths), str(backup_paths))
    manifest = json.loads((HOME / '.config/agent-home/manifest.json').read_text())
    check('manifest targets absolute and isolated', all(Path(x['target']).is_absolute() and Path(x['target']).is_relative_to(HOME) for x in manifest['entries'].values()))
    run('status-installed', [BIN, 'status'])
    run('resolved-config', [BIN, 'config', 'list', '--json'])
    doctor, doctor_code = run('doctor', [BIN, 'doctor', '--home-repo', SHARED, '--profile', 'core', '--json'], expected=None)
    observe('doctor result in intentionally partial tool environment', doctor_code == 0, doctor[-1500:])
    before_repeat = snap('before-repeat-install')
    run('repeat-install', install)
    after_repeat = snap('after-repeat-install')
    excluded = '.config/agent-home/manifest.json'
    check('repeat install unchanged except manifest', {k:v for k,v in before_repeat.items() if k != excluded} == {k:v for k,v in after_repeat.items() if k != excluded})
    run('vault-check', [BIN, 'vault', 'check', '--path', VAULT])
    run('ledger-note', [BIN, 'vault', 'new-note', '--path', VAULT, '--task', 'dogfood', '--agent', 'isolated', '--date', '2026-11-25'])
    run('vault-snapshot', [BIN, 'vault', 'snapshot', '--path', VAULT, '--message', 'Disposable personal snapshot'])
    tracked = git('vault-tracked', ['ls-files'], VAULT)
    check('vault snapshot excludes Team', not any(x == 'Team' or x.startswith('Team/') for x in tracked.splitlines()))
    check('vault has no production remote', git('vault-remotes', ['remote', '-v'], VAULT) == '')
    project = REPOS / 'sample-app'
    for tool in ['opencode', 'copilot', 'claude', 'codex']:
        run(tool + '-version', [tool, '--version'])
    listing, code = run('opencode-roles', ['strace', '-f', '-qq', '-e', 'trace=openat,newfstatat,statx', '-o', E / 'opencode-roles.trace', 'opencode', 'agent', 'list'], cwd=project, expected=None)
    observe('real OpenCode role discovery', code == 0 and 'ex-code-reviewer' in listing, 'opencode agent list; no model call')
    skills, code = run('opencode-skills', ['strace', '-f', '-qq', '-e', 'trace=openat,newfstatat,statx', '-o', E / 'opencode-skills.trace', 'opencode', 'debug', 'skill'], cwd=project, expected=None)
    observe('real OpenCode skill discovery', code == 0 and 'demo-skill' in skills, 'opencode debug skill; no model call')
    resolved, code = run('opencode-config', ['opencode', 'debug', 'config'], cwd=project, expected=None)
    observe('real OpenCode instruction configuration', code == 0 and str(SHARED / 'AGENTS.md') in resolved and str(VAULT / 'AGENTS.md') in resolved and '~/owner-rules.md' in resolved)
    generated_env, _ = run('generated-shell-environment', ['bash', '--noprofile', '--norc', '-c', '. "$HOME/.config/agent-home/config.env"; env -0'])
    cop_env = dict(x.split('=',1) for x in generated_env.split('\0') if '=' in x)
    check('product exports exact Copilot directories', cop_env.get('COPILOT_CUSTOM_INSTRUCTIONS_DIRS') == str(SHARED) + ',' + str(VAULT))
    prompt, code = run('codex-prompt-input', ['strace', '-f', '-qq', '-e', 'trace=openat,newfstatat,statx', '-o', E / 'codex-prompt.trace', 'codex', 'debug', 'prompt-input'], cwd=project, expected=None)
    observe('Codex owner instructions consumed', code == 0 and 'Preserve CRLF and no final newline' in prompt)
    observe('Codex installed shared skill discovery', code == 0 and 'demo-skill' in prompt)
    observe('Codex shared import content expanded', code == 0 and 'SHARED-RULE-DOGFOOD-MARKER' in prompt, 'Literal @import paths alone do not establish content expansion.')
    observe('Claude authenticated instruction/role discovery', False, 'Not attempted: --version alone does not consume rules; signed-in model use excluded.')
    instructions, code = run('copilot-instructions', ['strace', '-f', '-qq', '-e', 'trace=openat,newfstatat,statx', '-o', E / 'copilot-instructions.trace', 'copilot', 'instruction', 'list', '--json'], cwd=project, expected=None, env=cop_env)
    observe('real Copilot instruction discovery', code == 0 and str(SHARED / 'AGENTS.md') in instructions and str(VAULT / 'AGENTS.md') in instructions)
    skills, code = run('copilot-skills', ['copilot', 'skill', 'list'], cwd=project, expected=None, env=cop_env)
    observe('real Copilot skill discovery', code == 0 and 'demo-skill' in skills)
    control = SHARED / 'dogfood-control.instructions.md'
    put(control, '# Positive control\nCONTROL-DOGFOOD-INSTRUCTION\n')
    controlled, code = run('copilot-positive-control', ['copilot', 'instruction', 'list', '--json'], cwd=project, expected=None, env=cop_env)
    observe('Copilot external instruction positive control', code == 0 and 'dogfood-control.instructions.md' in controlled)
    control.unlink()
    check('installation and agent probes preserve shared content', snapshot(SHARED) == shared_before)
    note = VAULT / 'Inbox/cache.md'
    put(note, '---\ntitle: Cache proposal\ncreated: 2026-11-25\nupdated: 2026-11-25\ntype: note\narea: general\ntags: [architecture]\n---\nUse [[Team/Standards/coding-standards]] for review.\n')
    put(VAULT / 'Projects/consumer.md', '---\ntitle: Consumer\ncreated: 2026-11-25\nupdated: 2026-11-25\ntype: note\narea: general\ntags: [architecture]\n---\nRead [[Inbox/cache#Heading|proposal]].\n')
    share = [BIN, 'share', note, '--area', 'Architecture', '--owner', 'Isolated Tester']
    before_share = snap('before-share-preview')
    run('share-preview', share)
    check('share preview zero mutation', before_share == snap('after-share-preview'))
    main_head = git('shared-main-before-share', ['rev-parse', 'main'], SHARED)
    run('share-apply', share + ['--yes'])
    check('promoted note identity rewritten', '[[Standards/coding-standards]]' in (SHARED / 'Knowledge/Architecture/cache.md').read_text())
    check('inbound personal link rewritten', 'Team/Knowledge/Architecture/cache#Heading|proposal' in (VAULT / 'Projects/consumer.md').read_text())
    check('personal source retained', note.exists())
    check('share changes branch not trunk', git('shared-main-after-share', ['rev-parse', 'main'], SHARED) == main_head and git('shared-branch-after-share', ['branch', '--show-current'], SHARED) == 'share/cache')
    before_refuse = snap('before-share-collision')
    refusal, code = run('share-collision', share + ['--yes'], expected=None)
    check('collision refused', code != 0 and 'already exists' in refusal)
    check('collision preserved exact state', before_refuse == snap('after-share-collision'))
    (SHARED / 'Agents/Skills/added-dogfood').mkdir()
    put(SHARED / 'Agents/Skills/added-dogfood/SKILL.md', '---\nname: added-dogfood\ndescription: Use when testing relink.\n---\n# Disposable relink skill\n')
    before_update = snap('before-update-preview')
    run('update-preview', [BIN, 'update', '--tag', TAG, '--dry-run'])
    check('update preview zero mutation', before_update == snap('after-update-preview'))
    run('update-relink', [BIN, 'update', '--tag', TAG])
    check('equal version relink uses recorded vault and new skill', (HOME / '.agents/skills/added-dogfood').resolve() == SHARED / 'Agents/Skills/added-dogfood' and (VAULT / 'Team').resolve() == SHARED and not (HOME / 'Team').exists())
    exports_after, _ = run('relink-generated-environment', ['bash', '--noprofile', '--norc', '-c', '. "$HOME/.config/agent-home/config.env"; printf "%s" "${EX_REPOS-unset}"'])
    product_check('relink retains recorded repository-directory export', exports_after == str(REPOS), 'Expected '+str(REPOS)+'; observed '+exports_after)
    config_after, _ = run('relink-resolved-config', [BIN, 'config', 'list', '--json'])
    product_check('relink config resolves recorded repos', str(REPOS) in config_after, config_after)
    replacement = HOME / '.local/bin/repaired-vesica'
    put(replacement, b'#!/bin/sh\nprintf "not vesica\\n"\n', 0o755)
    before_repair = snap('before-artifact-repair-preview')
    release_env = {'VESICA_RELEASE_BASE_URL': (BASE / 'release-endpoint').as_uri()}
    run('artifact-repair-preview', [BIN, 'update', '--tag', TAG, '--target', replacement, '--dry-run'], env=release_env)
    check('artifact repair preview preserves target and config', before_repair == snap('after-artifact-repair-preview'))
    run('artifact-repair', [BIN, 'update', '--tag', TAG, '--target', replacement], env=release_env)
    check('actual artifact replacement matches fixed build', digest(replacement) == digest(SOURCE / 'target/x86_64-unknown-linux-musl/release/vesica') and stat.S_IMODE(replacement.stat().st_mode) == 0o755)
    check('no retained successful staging or recovery', not list((HOME / '.local/bin').glob('.vesica-*')) and not list((HOME / '.config/agent-home').glob('.vesica-relink-*')))
    run('vault-check-after-share', [BIN, 'vault', 'check', '--path', VAULT])
    run('vault-scan', [BIN, 'vault-scan', '--path', VAULT])
    run('vault-snapshot-final', [BIN, 'vault', 'snapshot', '--path', VAULT, '--message', 'Disposable post-promotion snapshot'])
    before_uninstall = snap('before-uninstall-preview')
    run('uninstall-preview', [BIN, 'uninstall', '--dry-run'])
    check('uninstall preview zero mutation', before_uninstall == snap('after-uninstall-preview'))
    shared_before_uninstall = snap('shared-before-uninstall', SHARED)
    vault_before_uninstall = snap('vault-before-uninstall', VAULT)
    repo_before_uninstall = snap('repos-before-uninstall', REPOS)
    uninstall_inventory = json.loads((HOME / '.config/agent-home/manifest.json').read_text())['entries']
    run('uninstall', [BIN, 'uninstall'])
    remaining = json.loads((HOME / '.config/agent-home/manifest.json').read_text())['entries']
    check('uninstall consumes every managed entry', not remaining, str(remaining))
    for key, entry in uninstall_inventory.items():
        target = Path(entry['target'])
        if entry.get('backup'):
            check('uninstall restored regular original '+key, target.is_file() and not target.is_symlink())
            check('uninstall consumed backup '+key, not Path(entry['backup']).exists() and not Path(entry['backup']).is_symlink())
        else:
            check('uninstall removed managed target '+key, not target.exists() and not target.is_symlink())
    for p, data in seed_originals.items():
        check('owner exact restoration ' + p, (HOME / p).read_bytes() == data)
    check('owner link referent preserved', (HOME / 'owner-link').is_symlink() and (HOME / 'owner-dir/sentinel.txt').read_text() == 'unrelated internal link referent\n')
    check('vault Team removed only', {k:v for k,v in vault_before_uninstall.items() if k != 'Team'} == snapshot(VAULT))
    check('promoted shared home preserved by uninstall', shared_before_uninstall == snapshot(SHARED))
    check('cloned code repos preserved by uninstall', repo_before_uninstall == snapshot(REPOS))
    run('status-uninstalled', [BIN, 'status'])
    before_second_uninstall = snap('before-second-uninstall')
    run('second-uninstall', [BIN, 'uninstall'])
    check('second uninstall idempotent', before_second_uninstall == snap('after-second-uninstall'))
    check('fixed product source unchanged', source_before == snap('source-after', SOURCE))
    snap('final-home')
    archive = E / 'disposable-home.tar.gz'
    run('archive-home', ['tar', '-czf', archive, '--exclude=.npm-global', '--exclude=.cache', '--exclude=.local/share', '-C', HOME, '.'])
    (E / 'DONE').write_text('completed\n')
    if any(x['status'] == 'FAIL' for x in rows):
        raise AssertionError('Completed walkthrough with recorded product failures; see results.json')


try:
    main()
except BaseException as error:
    (E / 'failure.txt').write_text(repr(error) + '\n')
    import traceback
    traceback.print_exc()
    save()
    sys.exit(1)
finally:
    (E / 'evidence-sha256.json').write_text(json.dumps({str(p.relative_to(E)):digest(p) for p in sorted(E.rglob('*')) if p.is_file() and p.name != 'evidence-sha256.json'}, indent=2))
