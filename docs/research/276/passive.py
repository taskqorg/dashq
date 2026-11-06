#!/usr/bin/env python3
"""Linux passive wait-class sampler; attaches to a PM-owned probe, never signals it."""
import argparse
import collections
import json
import os
from pathlib import Path
import time

# Exact kernel symbols only. Never export a symbol supplied by the target/kernel.
WAITS = {
    'wait_for_partner': 'fifo_open',
    'pipe_read': 'pipe_read', 'pipe_write': 'pipe_write',
    'futex_wait_queue': 'futex', 'futex_wait_queue_me': 'futex',
    'futex_wait': 'futex', 'do_futex': 'futex',
    'do_wait': 'child_wait', 'kernel_wait4': 'child_wait',
    'hrtimer_nanosleep': 'timer', 'do_nanosleep': 'timer',
    'io_schedule': 'io_wait', 'folio_wait_bit_common': 'io_wait',
    'wait_on_page_bit_common': 'io_wait',
    'do_epoll_wait': 'poll', 'ep_poll': 'poll',
    'poll_schedule_timeout.constprop.0': 'poll',
}
STATES = {'R': 'runnable', 'S': 'sleeping', 'D': 'uninterruptible',
          'T': 'stopped', 't': 'stopped', 'Z': 'exited', 'X': 'exited', 'I': 'idle'}


def stat(path):
    # /proc stat's comm can contain spaces and parentheses; discard it completely.
    tail = path.read_text()[:8192].rsplit(')', 1)[1].split()
    return tail[0], int(tail[11]) + int(tail[12]), int(tail[19])


def wait_class(state, symbol):
    if state == 'R':
        return 'runnable'
    if state in ('Z', 'X'):
        return 'exited'
    if symbol in ('', '0'):
        return 'unavailable'
    return WAITS.get(symbol, 'other_wait')


def collect(pid, seconds=20, root=Path('/proc'), interval=0.25):
    if not (pid > 0 and 0 < seconds <= 25 and 0.01 <= interval <= 1):
        raise ValueError('invalid bounds')
    base = root / str(pid)
    states, waits = collections.Counter(), collections.Counter()
    previous = {}
    ticks = samples = missing = capped = 0
    end_reason = 'deadline'
    began = time.monotonic()
    try:
        identity = stat(base / 'stat')[2]
    except (OSError, ValueError, IndexError):
        return {'schema': 'dashq-passive/1', 'state': 'unknown', 'reason': 'target_unavailable'}
    while time.monotonic() - began < seconds:
        try:
            leader = stat(base / 'stat')
            if leader[2] != identity:
                end_reason = 'identity_changed'
                break
            if leader[0] in ('Z', 'X'):
                end_reason = 'target_exited'
                break
            # Enumerate at most 257 entries, never read cmdline, environ, fd, maps or stack.
            tids = []
            with os.scandir(base / 'task') as entries:
                for entry in entries:
                    if entry.name.isdecimal():
                        tids.append(entry.name)
                        if len(tids) == 257:
                            break
            if len(tids) > 256:
                capped += 1
            for tid in tids[:256]:
                task = base / 'task' / tid
                try:
                    state, cpu, birth = stat(task / 'stat')
                    # Bounded read; no kernel-provided name ever leaves wait_class().
                    with (task / 'wchan').open() as stream:
                        symbol = stream.read(128).strip()
                    key = (tid, birth)
                    if key in previous:
                        ticks += max(0, cpu - previous[key])
                    previous[key] = cpu
                    states[STATES.get(state, 'unknown')] += 1
                    waits[wait_class(state, symbol)] += 1
                    samples += 1
                except (OSError, ValueError, IndexError):
                    missing += 1
            # Thread churn cannot grow retained state without bound.
            previous = {k: v for k, v in previous.items() if k[0] in tids[:256]}
        except (OSError, ValueError, IndexError):
            end_reason = 'target_unavailable'
            break
        time.sleep(min(interval, max(0, seconds - (time.monotonic() - began))))
    coverage = ('unknown' if not samples else
                'incomplete' if missing or capped or waits['unavailable'] or waits['other_wait'] or end_reason == 'identity_changed'
                else 'observed')
    return {'schema': 'dashq-passive/1', 'state': coverage, 'end': end_reason,
            'elapsed_ms': round((time.monotonic() - began) * 1000),
            'thread_samples': samples, 'missing_samples': missing, 'capped_polls': capped,
            'cpu_ms': round(ticks * 1000 / os.sysconf('SC_CLK_TCK')),
            'states': dict(states), 'wait_classes': dict(waits)}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pid', type=int)
    parser.add_argument('--seconds', type=float, default=20)
    args = parser.parse_args()
    try:
        print(json.dumps(collect(args.pid, args.seconds)))
    except Exception:
        print('{"schema":"dashq-passive/1","state":"unknown","reason":"collection_failed"}')
        raise SystemExit(1)
