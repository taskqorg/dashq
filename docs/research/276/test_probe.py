"""A3 / R1,R7: protect the probe's metadata-only export and deadline cleanup."""
import contextlib
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
import phases


class Probe(unittest.TestCase):
    def test_drops_output_errors_and_unapproved_phase_fields(self):
        with tempfile.TemporaryDirectory() as root:
            binary = Path(root) / 'invented'
            binary.write_text('''#!/usr/bin/env python3
import os
os.write(1, b'invented-private-report')
os.write(2, b'invented-private-stderr')
fd = int(os.environ['DASHQ_PHASE_FD'])
os.write(fd, b'{"phase":"build","ms":3}\\n')
os.write(fd, b'{"phase":"invented-private-label","ms":4}\\n')
os.write(fd, b'{"phase":"build","ms":4,"private":"invented-private-field"}\\n')
os.write(fd, b'{"phase":"build","ms":-1}\\n')
''')
            binary.chmod(0o700)
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                result = phases.run(binary, 1)
            self.assertEqual(result['phases'], [{'phase': 'build', 'ms': 3}])
            self.assertEqual(result['stdout_bytes'], len(b'invented-private-report'))
            self.assertNotIn('invented-private', output.getvalue())
            self.assertTrue(result['collection_complete'])
            self.assertEqual(json.loads(output.getvalue()), result)

    def test_deadline_kills_dedicated_descendants_and_reaps_parent(self):
        with tempfile.TemporaryDirectory() as root:
            binary = Path(root) / 'invented'
            binary.write_text('''#!/usr/bin/env python3
import os, time
if os.fork() == 0:
    time.sleep(60)
time.sleep(60)
''')
            binary.chmod(0o700)
            with contextlib.redirect_stdout(io.StringIO()):
                result = phases.run(binary, 0.1)
            self.assertTrue(result['timed_out'])
            self.assertEqual(result['exit_code'], -9)
            self.assertTrue(result['collection_complete'])
            self.assertLess(result['elapsed_ms'], 2000)


if __name__ == '__main__':
    unittest.main()
