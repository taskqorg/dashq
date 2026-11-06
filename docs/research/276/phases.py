#!/usr/bin/env python3
"""Research-only Unix phase probe. No report/stderr/argv/config data is exported."""
import argparse
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import tarfile
import tempfile
import threading
import time

REFS = {'0.18.1': 'bfe39ebfdab6559d0d0922d6e8545c93bf883138',
        '0.18.2-candidate': '55e8cbe06d59441c29155a3e9126611f78923314'}
RUST = r'''
fn research_phase(label: &'static str) {
    use std::io::Write;
    use std::os::fd::FromRawFd;
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let ms = START.get_or_init(std::time::Instant::now).elapsed().as_millis();
    let Ok(fd) = std::env::var("DASHQ_PHASE_FD").unwrap_or_default().parse::<i32>() else { return };
    if fd < 3 { return }
    let _lock = LOCK.lock();
    // Borrow the dedicated inherited descriptor; never close it or touch stderr.
    let mut f = std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(fd) });
    let _ = writeln!(f, "{{\"phase\":\"{}\",\"ms\":{}}}", label, ms);
}
'''
# Boundaries are literal and checked once; unsupported source fails closed.
MARKERS = [
    ('        let c = &self.cfg;\n        let now = source::now();', 'sources'),
    ('        let serverq = if c.serverq_report.is_empty()', 'serverq'),
    ('        let agent_work = if c.taskq_logs.is_empty()', 'tasklog'),
    ('        let projection = enroll::projection(', 'projection'),
    ('        let enrolled = projection.as_ref()', 'resources'),
    ('        let findings = projection.as_ref()', 'findings'),
    ('        let stages = projection.as_ref()', 'stages'),
    ('        let fleet_records = projection.as_ref()', 'fleet'),
    ('        let cli_releases: serde_json::Map', 'cli_releases'),
    ('        let doc = data::build(', 'build'),
    ('        if let Some(id) = doc["filters"]["block"]', 'block'),
    ('        if let (Some(store::Scope::All), Ok(e)) = (scope, &self.enroll)', 'export_status'),
]
OPTIONAL = [('        let server_inventory = projection.as_ref()', 'inventory'),
            ('        let observations = projection.as_ref()', 'observations'),
            ('        self.assets(&mut doc, scope, now, background);', 'assets')]


def prepare(version, dest):
    dest = Path(dest).resolve()
    if dest.exists():
        raise ValueError('destination must not exist')
    dest.mkdir(mode=0o700)
    data = subprocess.check_output(['git', 'archive', REFS[version]], stderr=subprocess.DEVNULL)
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        archive.extractall(dest, filter='data')
    path = dest / 'src/main.rs'
    src = path.read_text()
    for marker, label in MARKERS:
        if src.count(marker) != 1:
            raise ValueError('unsupported source boundary')
        src = src.replace(marker, f'        research_phase("{label}");\n' + marker)
    for marker, label in OPTIONAL:
        if marker in src:
            if src.count(marker) != 1:
                raise ValueError('ambiguous source boundary')
            src = src.replace(marker, f'        research_phase("{label}");\n' + marker)
    marker = '            let (period, filters) = report_query(rest)'
    if src.count(marker) != 1:
        raise ValueError('unsupported report entry')
    src = src.replace(marker, '            research_phase("config");\n' + marker)
    marker = '            let enroll = enroll::Enroll::open_with(&dir, &cfg.public_host, cfg.limits);'
    if src.count(marker) != 1:
        raise ValueError('unsupported store entry')
    src = src.replace(marker, '            research_phase("store_open");\n' + marker)
    # Separate document generation from formatting and output, preserving its arguments.
    start = src.index('            println!("{}", App { cfg, cache:', src.index('["report", rest @ ..]'))
    end = src.index('\n', start)
    line = src[start:end]
    expression = line[len('            println!("{}", '):-2]
    src = src[:start] + ('            let document = ' + expression + ';\n'
        '            research_phase("serialize_output");\n'
        '            println!("{}", document);\n'
        '            research_phase("done");') + src[end:]
    path.write_text(src + RUST)
    print(json.dumps({'prepared': version, 'source_sha': REFS[version]}))


def run(binary, seconds):
    # The caller supplies authorization and environment. Never inspect config or process argv.
    read_fd, write_fd = os.pipe()
    records = []
    allowed = {label for _, label in MARKERS + OPTIONAL} | {'config', 'store_open', 'serialize_output', 'done'}
    def phases():
        with os.fdopen(read_fd, 'r') as stream:
            for i, line in enumerate(stream):
                if i >= 64 or len(line) > 128:
                    continue
                try:
                    row = json.loads(line)
                    if set(row) == {'phase', 'ms'} and row['phase'] in allowed and type(row['ms']) is int and 0 <= row['ms'] <= 120000:
                        records.append(row)
                except (ValueError, TypeError):
                    pass
    env = dict(os.environ, DASHQ_PHASE_FD=str(write_fd))
    began = time.monotonic()
    proc = subprocess.Popen([str(Path(binary).resolve()), 'report'], env=env,
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, pass_fds=(write_fd,), start_new_session=True)
    os.close(write_fd)
    count = [0]
    def discard():
        while block := proc.stdout.read(65536):
            count[0] += len(block)
        proc.stdout.close()
    readers = [threading.Thread(target=phases), threading.Thread(target=discard)]
    for thread in readers:
        thread.start()
    timed_out = False
    try:
        proc.wait(timeout=seconds)
    except subprocess.TimeoutExpired:
        timed_out = True
    finally:
        # Always clean this dedicated group, including any surviving descendants.
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait()
        for thread in readers:
            thread.join(timeout=2)
    result = {'elapsed_ms': round((time.monotonic() - began) * 1000),
              'deadline_ms': round(seconds * 1000), 'timed_out': timed_out,
              'exit_code': proc.returncode, 'stdout_bytes': count[0], 'phases': records,
              'collection_complete': all(not t.is_alive() for t in readers)}
    print(json.dumps(result))
    return result


def synthetic(binary):
    # No network, key, database or live config. Only a private disposable FIFO and JSON file.
    with tempfile.TemporaryDirectory(prefix='dashq276-') as root:
        root = Path(root)
        fifo = root / 'invented-report'
        os.mkfifo(fifo, 0o600)
        config = root / 'config.json'
        config.write_text(json.dumps({'public_host': 'dash.example.com',
            'serverq': {'report': str(fifo), 'host': 'invented.example/native'}}))
        config.chmod(0o600)
        previous = os.environ.get('DASHQ_CONFIG')
        os.environ['DASHQ_CONFIG'] = str(config)
        try:
            blocked = run(binary, 1)
            assert blocked['timed_out'] and blocked['stdout_bytes'] == 0 and blocked['collection_complete']
            assert blocked['phases'][-1]['phase'] == 'serverq'
            # Replace only synthetic input with a regular empty report, then repeat.
            fifo.unlink()
            fifo.write_text('{"schema":1,"at":1800000000,"results":{}}')
            completed = run(binary, 3)
            assert not completed['timed_out'] and completed['exit_code'] == 0 and completed['collection_complete']
            assert completed['phases'][-1]['phase'] == 'done' and completed['stdout_bytes'] > 0
        finally:
            if previous is None:
                os.environ.pop('DASHQ_CONFIG', None)
            else:
                os.environ['DASHQ_CONFIG'] = previous


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    p = sub.add_parser('prepare')
    p.add_argument('version', choices=REFS)
    p.add_argument('destination')
    p = sub.add_parser('run')
    p.add_argument('binary')
    p.add_argument('--seconds', type=float, default=30)
    p = sub.add_parser('synthetic')
    p.add_argument('binary')
    args = parser.parse_args()
    try:
        if args.action == 'prepare':
            prepare(args.version, args.destination)
        elif args.action == 'synthetic':
            synthetic(args.binary)
        elif 0 < args.seconds <= 30:
            run(args.binary, args.seconds)
        else:
            parser.error('deadline must be greater than zero and at most 30 seconds')
    except Exception:
        # Exceptions can contain paths or other private input; export only a fixed class.
        print('{"error":"probe_failed"}')
        raise SystemExit(1)
