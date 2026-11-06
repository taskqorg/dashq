#!/usr/bin/env python3
"""Synthetic-only report CPU experiment: invented stage log, no store or network."""
import argparse
import contextlib
import io
import json
import os
from pathlib import Path
import resource
import tempfile
import time
import phases


def experiment(binary, count):
    if count not in (10000, 100000):
        raise ValueError('unsupported fixture size')
    with tempfile.TemporaryDirectory(prefix='dashq276-cpu-') as directory:
        root = Path(directory)
        log = root / 'invented.log'
        now = int(time.time()) * 1000
        # Same terminal stage, distinct attempt identities; all fields are invented metadata.
        with log.open('w') as stream:
            for number in range(1, count + 1):
                stream.write('taskq-stage ' + json.dumps({'stage': 'build', 'task': number,
                    'attempt': number, 'rt': 'claude', 'us': 1000, 'outcome': 'ok',
                    'error': 'none', 'at_ms': now, 'calls': 1, 'tokens': 1,
                    'output_tokens': 1, 'model': 'invented', 'project': 'invented/project'}) + '\n')
        config = root / 'config.json'
        config.write_text(json.dumps({'public_host': 'dash.example.com',
                                     'taskq_logs': {'invented': str(log)}}))
        config.chmod(0o600)
        old = os.environ.get('DASHQ_CONFIG')
        os.environ['DASHQ_CONFIG'] = str(config)
        before = resource.getrusage(resource.RUSAGE_CHILDREN)
        try:
            with contextlib.redirect_stdout(io.StringIO()):
                result = phases.run(binary, 30)
        finally:
            if old is None:
                os.environ.pop('DASHQ_CONFIG', None)
            else:
                os.environ['DASHQ_CONFIG'] = old
        after = resource.getrusage(resource.RUSAGE_CHILDREN)
        result.update({'synthetic_rows': count, 'synthetic_bytes': log.stat().st_size,
                       'child_cpu_ms': round(1000 * (after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime))})
        print(json.dumps(result))
        if not result['collection_complete']:
            raise RuntimeError('incomplete cleanup')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary')
    parser.add_argument('--rows', type=int, choices=(10000, 100000), required=True)
    args = parser.parse_args()
    try:
        experiment(args.binary, args.rows)
    except Exception:
        print('{"error":"synthetic_experiment_failed"}')
        raise SystemExit(1)
