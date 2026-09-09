"""Production CLI restore/retry/ownership regressions with recording executables."""

import json
import fcntl
from pathlib import Path
import unittest

import test_crew_staging as fixture


class RestoreFlowTests(unittest.TestCase):
    run_cli = fixture.PlanTests.run_cli
    calls = fixture.PlanTests.calls
    server_config = fixture.PlanTests.server_config

    def setUp(self):
        fixture.SnapshotTests.setUp(self)
        self.config['source']['community_id'] = '11111111-1111-1111-1111-111111111111'
        self.ids = {name: str(index) * 64 for index, name in enumerate(('relay', 'postgres', 'redis', 'media'), 1)}
        self.config['target']['container_ids'] = self.ids
        self.config['target']['network_id'] = 'a' * 64
        self.config['target']['volume_mounts'] = {name: '/var/lib/docker/volumes/' + volume + '/_data'
                                                for name, volume in self.config['target']['volumes'].items()}
        self.target = self.root / 'target'
        self.target.mkdir()
        self.items = []
        for name, identifier in self.ids.items():
            volumes = [name] if name != 'relay' else ['git']
            if name == 'postgres':
                volumes += ['media', 'git']
            self.items.append({
                'Id': identifier, 'State': {'Running': False},
                'Config': {'Image': self.config['images'][name],
                           'Labels': {'com.nuncio.crew.staging': self.config['environment_id']},
                           'Env': ['DATABASE_URL=postgres://staging:secret@postgres:5432/crew_staging_test',
                                   'RELAY_URL=ws://100.86.143.13:3348',
                                   'REDIS_URL=redis://redis:6379', 'BUZZ_S3_ENDPOINT=http://media:9000',
                                   'BUZZ_REQUIRE_AUTH_TOKEN=true', 'BUZZ_REQUIRE_RELAY_MEMBERSHIP=true',
                                   'BUZZ_AUTO_MIGRATE=false', 'BUZZ_PUSH_ENABLED=false',
                                   'RELAY_OWNER_PUBKEY=' + 'a' * 64]},
                'HostConfig': {'NetworkMode': self.config['target']['network'],
                               'NanoCpus': 1000000000, 'Memory': 536870912},
                'NetworkSettings': {'Networks': {self.config['target']['network']: {'NetworkID': 'a' * 64}}},
                'Mounts': [{'Type': 'volume', 'RW': True, 'Name': self.config['target']['volumes'][v],
                            'Source': self.config['target']['volume_mounts'][v],
                            'Destination': ('/restore/' + v if v != 'postgres' else '/var/lib/postgresql/data')
                            if name == 'postgres' else '/srv/git' if name == 'relay' else '/data'}
                           for v in volumes],
            })
        self.items[1]['Mounts'].append({'Type': 'bind', 'RW': False,
                                      'Source': str(self.target / 'secrets/postgres-password'),
                                      'Destination': '/run/staging/postgres-password'})
        for name, item in zip(self.ids, self.items):
            bindings = ({'5432/tcp': [{'HostIp': '127.0.0.1', 'HostPort': '15348'}]} if name == 'postgres'
                        else {'3000/tcp': [{'HostIp': '100.86.143.13', 'HostPort': '3348'}],
                              '8080/tcp': [{'HostIp': '127.0.0.1', 'HostPort': '8348'}],
                              '9102/tcp': [{'HostIp': '127.0.0.1', 'HostPort': '9348'}]} if name == 'relay' else {})
            item['HostConfig']['PortBindings'] = bindings
            item['NetworkSettings']['Ports'] = bindings
        self.inspect = self.root / 'inspect.json'
        self.inspect.write_text(json.dumps(self.items))
        self.env['INSPECT_RESULT'] = str(self.inspect)
        for key, value in {
            'VOLUME_INSPECT': [{'Name': volume, 'Mountpoint': self.config['target']['volume_mounts'][name],
                                'Driver': 'local', 'Options': None,
                                'Labels': {'com.nuncio.crew.staging': self.config['environment_id']}}
                               for name, volume in self.config['target']['volumes'].items()],
            'NETWORK_INSPECT': [{'Id': 'a' * 64, 'Name': self.config['target']['network'], 'Driver': 'bridge',
                                 'Labels': {'com.nuncio.crew.staging': self.config['environment_id']}}],
        }.items():
            file = self.root / (key + '.json')
            file.write_text(json.dumps(value))
            self.env[key] = str(file)
        self.assertEqual(self.run_cli('snapshot', 'fixed').returncode, 0)

    def test_restore_replays_fixed_baseline_and_never_reseeds(self):
        baseline = (self.root / 'baselines/fixed/manifest.json').read_bytes()
        for _ in range(2):
            result = self.run_cli('restore', 'fixed')
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)['status'], 'RESTORED_UNVERIFIED')
        self.assertEqual((self.root / 'baselines/fixed/manifest.json').read_bytes(), baseline)
        drops = [call for call in self.calls() if 'dropdb' in call]
        self.assertEqual(len(drops), 2)
        self.assertTrue(all(call[-1] == 'crew_staging_test' and self.ids['postgres'] in call for call in drops))

    def test_interrupted_restore_is_unavailable_without_relay_start(self):
        self.env['FAIL_COMMAND'] = 'pg_restore'
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(json.loads((self.target / 'state.json').read_text())['status'], 'UNAVAILABLE')
        self.assertFalse(any('start' in call and self.config['target']['container_ids']['relay'] in call for call in self.calls()))
        self.env.pop('FAIL_COMMAND')
        self.assertEqual(self.run_cli('restore', 'fixed').returncode, 0)

    def test_changed_owner_refuses_cleanup(self):
        self.items[0]['Config']['Labels']['com.nuncio.crew.staging'] = 'someone-else'
        self.inspect.write_text(json.dumps(self.items))
        result = self.run_cli('stop')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any('stop' in call for call in self.calls()))

    def test_restore_revokes_fixture_access_before_loading_data(self):
        result = self.run_cli('restore', 'fixed')
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        revoke = next((i for i, call in enumerate(calls)
                       if any('REVOKE CONNECT ON DATABASE' in arg for arg in call)), None)
        self.assertIsNotNone(revoke)
        load = next(i for i, call in enumerate(calls) if 'pg_restore' in call)
        self.assertLess(revoke, load)

    def test_git_archive_root_is_not_duplicated_on_restore(self):
        result = self.run_cli('restore', 'fixed')
        self.assertEqual(result.returncode, 0, result.stderr)
        extraction = next(call for call in self.calls() if 'tar' in call and '/tmp/git.tar' in call)
        self.assertEqual(extraction[extraction.index('-C') + 1], '/restore/git')

    def test_source_sql_and_dump_have_server_side_deadlines(self):
        source_calls = [call for call in self.calls()
                        if 'daily-postgres' in call and ('psql' in call or 'pg_dump' in call)]
        self.assertGreater(len(source_calls), 1)
        for call in source_calls:
            self.assertIn('timeout', call)
            self.assertTrue(any('default_transaction_read_only=on' in arg for arg in call))

    def test_same_volume_name_with_daily_actual_source_refused(self):
        self.items[1]['Mounts'][0]['Source'] = '/daily/postgres'
        self.inspect.write_text(json.dumps(self.items))
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any('stop' in call or 'dropdb' in call for call in self.calls()))

    def test_missing_or_retargeted_restore_mount_refused(self):
        original = json.loads(json.dumps(self.items))
        for remove in (True, False):
            self.items = json.loads(json.dumps(original))
            if remove:
                self.items[1]['Mounts'].pop()
            else:
                self.items[1]['Mounts'][1]['Destination'] = '/wrong-storage'
            self.inspect.write_text(json.dumps(self.items))
            result = self.run_cli('restore', 'fixed')
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertFalse(any('stop' in call for call in self.calls()))

    def test_one_retained_daily_environment_refuses_launch(self):
        self.items[0]['Config']['Env'][0] = 'DATABASE_URL=postgres://daily:secret@daily:5432/buzz'
        self.inspect.write_text(json.dumps(self.items))
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('secret', result.stdout + result.stderr)
        self.assertFalse(any('start' in call for call in self.calls()))

    def test_extra_daily_network_refused(self):
        self.items[0]['NetworkSettings']['Networks']['buzz-net'] = {'NetworkID': 'daily'}
        self.inspect.write_text(json.dumps(self.items))
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any('stop' in call for call in self.calls()))

    def test_unbounded_container_refused(self):
        self.items[0]['HostConfig']['Memory'] = 0
        self.inspect.write_text(json.dumps(self.items))
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any('stop' in call for call in self.calls()))

    def test_health_port_conflict_refuses_before_stopping_anything(self):
        self.env['LISTENERS'] = 'LISTEN 0 4096 127.0.0.1:8348 0.0.0.0:*'
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('8348', result.stderr)
        self.assertFalse(any('stop' in call for call in self.calls()))

    def test_running_owned_postgres_does_not_block_stopped_relay_restore(self):
        self.items[1]['State']['Running'] = True
        self.items[1]['HostConfig']['PortBindings'] = {'5432/tcp': [{'HostIp': '127.0.0.1', 'HostPort': '15348'}]}
        self.inspect.write_text(json.dumps(self.items))
        self.env['LISTENERS'] = 'LISTEN 0 4096 127.0.0.1:15348 0.0.0.0:*'
        result = self.run_cli('restore', 'fixed')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_volume_driver_alias_refused(self):
        file = Path(self.env['VOLUME_INSPECT'])
        volumes = json.loads(file.read_text())
        volumes[0]['Options'] = {'device': '/daily/postgres', 'type': 'none', 'o': 'bind'}
        file.write_text(json.dumps(volumes))
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any('stop' in call for call in self.calls()))

    def test_concurrent_restore_refused_before_mutation(self):
        with (self.target / '.crew-staging.lock').open('w') as lock:
            (self.target / '.crew-staging.lock').chmod(0o600)
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('operation already running', result.stderr)
        self.assertFalse(any('stop' in call for call in self.calls()))

    def test_fixed_baseline_can_be_reused_for_a_reviewed_new_build(self):
        self.config['build_sha'] = 'd' * 40
        self.config['images']['relay'] = 'relay@sha256:' + 'e' * 64
        self.compose['services']['relay']['image'] = self.config['images']['relay']
        self.items[0]['Config']['Image'] = self.config['images']['relay']
        self.inspect.write_text(json.dumps(self.items))
        result = self.run_cli('restore', 'fixed')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_oversized_baseline_metadata_refuses_before_target_inspection(self):
        file = self.root / 'baselines/fixed/manifest.json'
        with file.open('wb') as stream:
            stream.truncate(16777217)
        self.log.unlink()
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('metadata budget', result.stderr)
        self.assertEqual(self.calls(), [])

    def test_stale_temporary_file_does_not_wedge_restore_retry(self):
        (self.target / 'state.tmp').write_text('interrupted older writer')
        result = self.run_cli('restore', 'fixed')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_owned_pg_does_not_hide_unowned_health_listener(self):
        self.items[1]['State']['Running'] = True
        self.inspect.write_text(json.dumps(self.items))
        self.env['LISTENERS'] = 'LISTEN 0 4096 127.0.0.1:15348 0.0.0.0:*\nLISTEN 0 4096 127.0.0.1:8348 0.0.0.0:*'
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('8348', result.stderr)
        self.assertFalse(any('stop' in call for call in self.calls()))

    def test_new_stopped_container_may_have_unresolved_network_id(self):
        for item in self.items:
            item['NetworkSettings']['Networks'][self.config['target']['network']]['NetworkID'] = ''
        self.inspect.write_text(json.dumps(self.items))
        result = self.run_cli('restore', 'fixed')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_wrong_nonempty_or_running_empty_network_id_refused(self):
        for running, network_id in ((False, 'b' * 64), (True, '')):
            self.items[0]['State']['Running'] = running
            self.items[0]['NetworkSettings']['Networks'][self.config['target']['network']]['NetworkID'] = network_id
            self.inspect.write_text(json.dumps(self.items))
            result = self.run_cli('restore', 'fixed')
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(any('stop' in call or 'dropdb' in call for call in self.calls()))

    def test_unexpected_readiness_shape_stops_owned_relay(self):
        self.env['INVALID_READY_INSPECT'] = '1'
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        final_stop = [call for call in self.calls() if 'stop' in call][-1]
        self.assertEqual(final_stop[-1], self.ids['relay'])
        self.assertEqual(json.loads((self.target / 'state.json').read_text())['status'], 'UNAVAILABLE')

    def test_readiness_deadline_stops_owned_relay(self):
        self.env['READINESS_DELAY'] = '1000'
        self.config['limits']['timeout_seconds'] = 1
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('readiness deadline', result.stderr)
        self.assertEqual(json.loads((self.target / 'state.json').read_text())['status'], 'UNAVAILABLE')
        final_stop = [call for call in self.calls() if 'stop' in call][-1]
        self.assertIn(self.ids['relay'], final_stop)
        self.assertNotIn(self.ids['postgres'], final_stop)

    def test_relay_readiness_retries_transient_failure(self):
        self.env['READINESS_DELAY'] = '1'
        result = self.run_cli('restore', 'fixed')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertGreaterEqual(sum(call[0] == 'curl' for call in self.calls()), 2)

    def test_relay_exit_does_not_report_restored(self):
        self.env['RELAY_EXITS'] = '1'
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('relay exited', result.stderr)
        self.assertEqual(json.loads((self.target / 'state.json').read_text())['status'], 'UNAVAILABLE')

    def test_large_user_restore_suppresses_psql_command_tags(self):
        import hashlib
        folder = self.root / 'baselines/fixed'
        users = folder / 'users.json'
        users.write_text(json.dumps([{'about': 'x' * 33000}]))
        file = folder / 'manifest.json'
        manifest = json.loads(file.read_text())
        manifest['checksums']['users.json'] = hashlib.sha256(users.read_bytes()).hexdigest()
        file.write_text(json.dumps(manifest))
        result = self.run_cli('restore', 'fixed')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_deletion_state_blocks_relay_start(self):
        self.env['UNSAFE_STARTUP_STATE'] = '1'
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('community deletion', result.stderr)
        self.assertFalse(any('start' in call and self.config['target']['container_ids']['relay'] in call for call in self.calls()))

    def test_corrupted_persisted_asset_blocks_relay_start(self):
        self.env['CORRUPT_RESTORED_ASSET'] = '1'
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any('start' in call and self.config['target']['container_ids']['relay'] in call for call in self.calls()))


if __name__ == '__main__':
    unittest.main()
