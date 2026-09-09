"""A recorded target-only schema plan is required for old snapshots."""

import hashlib
import json
from pathlib import Path
import unittest

import test_crew_staging_restore as fixture


class MigrationTests(unittest.TestCase):
    run_cli = fixture.RestoreFlowTests.run_cli
    calls = fixture.RestoreFlowTests.calls
    server_config = fixture.RestoreFlowTests.server_config

    def setUp(self):
        fixture.RestoreFlowTests.setUp(self)
        self.config['source']['schema_version'] = '33'
        self.config['schema_version'] = '34'
        migration = Path(__file__).resolve().parents[2] / 'migrations/0034_private_managed_agent_fts.sql'
        self.config['migrations'] = [{'version': 34, 'path': str(migration),
                                     'sha256': hashlib.sha256(migration.read_bytes()).hexdigest()}]
        self.env['SOURCE_COUNTS'] = '{"event_count":1,"db_bytes":1000,"schema_version":"33","pending_reminders":0,"ephemeral_channels":0}'

    def test_recorded_schema_plan_restores_only_target(self):
        result = self.run_cli('snapshot', 'old-source')
        self.assertEqual(result.returncode, 0, result.stderr)
        result = self.run_cli('restore', 'old-source')
        self.assertEqual(result.returncode, 0, result.stderr)
        applies = [call for call in self.calls() if '--single-transaction' in call]
        self.assertEqual(len(applies), 1)
        self.assertIn(self.ids['postgres'], applies[0])
        self.assertIn('crew_staging_test', applies[0])

    def test_changed_migration_refuses_snapshot(self):
        self.config['migrations'][0]['sha256'] = '0' * 64
        before = self.calls()
        result = self.run_cli('snapshot', 'bad-migration')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('migration checksum', result.stderr)
        self.assertEqual(before, self.calls())

    def test_missing_migration_in_sequence_refuses(self):
        self.config['schema_version'] = '35'
        result = self.run_cli('snapshot', 'gap')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('migration sequence', result.stderr)


if __name__ == '__main__':
    unittest.main()
