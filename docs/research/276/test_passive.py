"""A3/R1/R7: passive filtering, identity protection and bounded observer lifetime."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import passive


class Passive(unittest.TestCase):
    def test_untrusted_names_and_pid_reuse_never_escape(self):
        # No prior check covered a /proc comm/wchan or target PID reuse.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            base = root / '123'
            task = base / 'task/123'
            task.mkdir(parents=True)
            def row(birth):
                tail = ['S'] + ['0'] * 20
                tail[11], tail[12], tail[19] = '4', '2', str(birth)
                return '123 (invented-private-name (x)) ' + ' '.join(tail)
            (base / 'stat').write_text(row(1))
            (task / 'stat').write_text(row(1))
            (task / 'wchan').write_text('invented-private-kernel-symbol')
            def reuse(_):
                (base / 'stat').write_text(row(2))
            with patch.object(passive.time, 'sleep', side_effect=reuse):
                result = passive.collect(123, 0.2, root)
            self.assertEqual(result['end'], 'identity_changed')
            self.assertEqual(result['wait_classes'], {'other_wait': 1})
            self.assertEqual(result['state'], 'incomplete')
            self.assertNotIn('invented-private', json.dumps(result))
            self.assertNotIn('123', json.dumps(result))
            self.assertEqual(passive.wait_class('S', '0'), 'unavailable')

    def test_synthetic_fifo_cpu_and_observer_exit_without_signalling_target(self):
        with tempfile.TemporaryDirectory() as directory:
            fifo = Path(directory) / 'invented'
            os.mkfifo(fifo, 0o600)
            for name, code in [('fifo', 'open(sys.argv[1]).read()'),
                               ('cpu', 'while True: pass')]:
                proc = subprocess.Popen([sys.executable, '-c', 'import sys\n' + code, str(fifo)],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                try:
                    result = passive.collect(proc.pid, 0.4, interval=0.05)
                    self.assertLess(result['elapsed_ms'], 1500)
                    self.assertIsNone(proc.poll(), 'observer must not terminate target')
                    self.assertGreater(result['thread_samples'], 0)
                    if name == 'fifo':
                        self.assertGreater(result['states'].get('sleeping', 0), 0)
                        self.assertTrue(set(result['wait_classes']) <= {'runnable', 'fifo_open', 'unavailable', 'other_wait'})
                    else:
                        self.assertGreater(result['cpu_ms'], 0)
                        self.assertGreater(result['wait_classes'].get('runnable', 0), 0)
                    print(json.dumps({'synthetic': name, 'observation': result}))
                finally:
                    # Only the synthetic test supervisor owns target cleanup.
                    proc.kill()
                    proc.wait()
            self.assertEqual(passive.collect(proc.pid, 0.1)['state'], 'unknown')


if __name__ == '__main__':
    unittest.main()
