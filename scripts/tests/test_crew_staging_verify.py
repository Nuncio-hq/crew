"""Verify must observe a real command write and exact persisted readback."""

import hashlib
import json
import unittest

import test_crew_staging_restore as fixture


class VerifyTests(unittest.TestCase):
    run_cli = fixture.RestoreFlowTests.run_cli
    calls = fixture.RestoreFlowTests.calls
    server_config = fixture.RestoreFlowTests.server_config

    def setUp(self):
        fixture.RestoreFlowTests.setUp(self)
        key = self.target / 'admin-key'
        key.write_text('b' * 64)
        key.chmod(0o600)
        cli = self.bin / 'buzz'
        self.config['probe'] = {'cli': str(cli), 'cli_sha256': hashlib.sha256(cli.read_bytes()).hexdigest(),
                                'admin_key_file': str(key)}
        self.env['PROBE_STATE'] = str(self.root / 'probe-event.json')
        self.assertEqual(self.run_cli('snapshot', 'verified').returncode, 0)
        self.assertEqual(self.run_cli('restore', 'verified').returncode, 0)
        for item in self.items:
            item['State']['Running'] = True
        self.inspect.write_text(json.dumps(self.items))

    def test_verify_requires_exact_write_readback(self):
        result = self.run_cli('verify', 'verified')
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt['status'], 'SERVER_VERIFIED')
        self.assertEqual(receipt['probe_event_id'], 'ab' * 32)
        self.assertEqual(receipt['auth_class'], 'real')
        self.assertTrue(any(call[0] == 'buzz' and 'send' in call for call in self.calls()))
        self.assertTrue(any(call[0] == 'buzz' and 'get' in call for call in self.calls()))

    def test_wrong_readback_never_reports_verified(self):
        self.env['CORRUPT_PROBE'] = '1'
        result = self.run_cli('verify', 'verified')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('readback', result.stderr)
        state = json.loads((self.target / 'state.json').read_text())
        self.assertNotEqual(state['status'], 'SERVER_VERIFIED')

    def test_verification_retry_reuses_persisted_probe(self):
        self.env['CORRUPT_PROBE'] = '1'
        self.assertNotEqual(self.run_cli('verify', 'verified').returncode, 0)
        del self.env['CORRUPT_PROBE']
        result = self.run_cli('verify', 'verified')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sum(call[0] == 'buzz' and 'send' in call for call in self.calls()), 1)
        self.assertEqual(sum(call[0] == 'buzz' and 'create' in call for call in self.calls()), 1)

    def test_unrestored_state_never_starts_probe(self):
        (self.target / 'state.json').write_text('{"status":"UNAVAILABLE"}')
        result = self.run_cli('verify', 'verified')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(call[0] == 'buzz' for call in self.calls()))


if __name__ == '__main__':
    unittest.main()
