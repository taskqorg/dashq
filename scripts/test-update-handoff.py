#!/usr/bin/env python3
"""D2 regression: old updater -> published staged binary, with all gates intact.
Pass directories containing downloaded 0.18.1 and 0.18.2 x86_64 release assets.
No installed service or network is used; curl delivers the supplied assets.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

old, new = [Path(p).resolve() for p in sys.argv[1:]]
for directory, version in [(old, '0.18.1'), (new, '0.18.2')]:
    name = f'dashq-{version}-linux-x86_64'
    digest = hashlib.sha256((directory / name).read_bytes()).hexdigest()
    assert f'{digest}  {name}' in (directory / 'SHA256SUMS').read_text()
with tempfile.TemporaryDirectory(prefix='dashq-handoff-') as temp:
    base = Path(temp)
    assets = base / 'assets'
    shutil.copytree(new, assets)
    name = 'dashq-0.18.2-linux-x86_64'
    (assets / 'latest.json').write_text(json.dumps({'version': '0.18.2', 'files': {'linux-x86_64': name}}))
    bins = base / 'bin'
    bins.mkdir()
    curl = bins / 'curl'
    curl.write_text('''#!/usr/bin/env python3
import os, shutil, sys
from pathlib import Path
a = sys.argv[1:]
shutil.copyfile(Path(os.environ['TEST_ASSETS']) / a[-1].rsplit('/', 1)[-1], a[a.index('-o')+1])
''')
    curl.chmod(0o755)
    env = dict(os.environ, HOME=str(base), PATH=f'{bins}:' + os.environ['PATH'], TEST_ASSETS=str(assets))
    env.pop('DASHQ_INSTALL', None)
    env.pop('DASHQ_CONFIG', None)

    def installation(label):
        root = base / label
        release = root / 'releases/0.18.1'
        release.mkdir(parents=True)
        shutil.copyfile(old / 'dashq-0.18.1-linux-x86_64', release / 'dashq')
        (release / 'dashq').chmod(0o755)
        shutil.copy(release / 'dashq', root / 'dashq')
        (root / 'current').write_text('0.18.1')
        return root

    def run(root, repaired):
        command = [str(Path(__file__).with_name('update-handoff.sh').resolve()), str(root)] if repaired else [str(root / 'dashq'), 'update']
        return subprocess.run(command, env=env, capture_output=True, text=True, timeout=60)

    root = installation('before')
    result = run(root, False)
    assert result.returncode != 0 and 'installation release entrypoint invalid' in result.stderr, result
    assert (root / 'current').read_text().strip() == '0.18.1'
    print('FAIL BEFORE: 0.18.1 -> 0.18.2: installation release entrypoint invalid')
    root = installation('after')
    result = run(root, True)
    assert result.returncode == 0, result
    assert (root / 'current').read_text().strip() == '0.18.2'
    assert (root / 'previous').read_text().strip() == '0.18.1'
    assert (root / 'releases/0.18.2/dashq').read_bytes() == (new / name).read_bytes()
    print('PASS AFTER: current 0.18.2; previous 0.18.1; published artifact byte-identical')
    (assets / 'SHA256SUMS').write_text('invalid checksum\n')
    root = installation('checksum')
    result = run(root, True)
    assert result.returncode != 0 and 'not in SHA256SUMS' in result.stderr, result
    assert (root / 'current').read_text().strip() == '0.18.1'
    print('PASS: checksum refusal preserves current')
    # Valid checksum for a failing candidate isolates the independent selftest guard.
    candidate = b'#!/bin/sh\necho invented-selftest-failure >&2\nexit 1\n'
    (assets / name).write_bytes(candidate)
    (assets / 'SHA256SUMS').write_text(f'{hashlib.sha256(candidate).hexdigest()}  {name}\n')
    root = installation('selftest')
    result = run(root, True)
    assert result.returncode != 0 and 'invented-selftest-failure' in result.stderr, result
    assert (root / 'current').read_text().strip() == '0.18.1'
    print('PASS: selftest refusal preserves current')
