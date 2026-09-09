"""Run the actual staging CLI; external commands are recording fakes."""

import json
import io
import os
from pathlib import Path
import subprocess
import socket
import sys
import tempfile
import tarfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
CLI = ROOT / "scripts/crew-staging.py"


class PlanTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "calls.jsonl"
        fake = """#!/usr/bin/env python3
import json, os, sys
with open(os.environ['CALL_LOG'], 'a') as f:
    f.write(json.dumps([os.path.basename(sys.argv[0]), *sys.argv[1:]]) + '\\n')
if os.path.basename(sys.argv[0]) == 'ss':
    print(os.environ.get('LISTENERS', ''))
    sys.exit(0)
if os.path.basename(sys.argv[0]) == 'curl':
    if os.environ.get('INVALID_HEALTH_SHAPE'):
        print('[]');sys.exit(0)
    count = sum('"curl"' in line for line in open(os.environ['CALL_LOG']).readlines())
    if count <= int(os.environ.get('READINESS_DELAY', '0')):
        sys.exit(22)
    print('{"status":"ready"}')
    sys.exit(0)
if os.path.basename(sys.argv[0]) == 'buzz':
    if 'create' in sys.argv:
        print('{"channel_id":"22222222-2222-2222-2222-222222222222"}')
    elif 'send' in sys.argv:
        event = {'id':'ab'*32, 'pubkey':'a'*64, 'content':sys.argv[sys.argv.index('--content')+1]}
        open(os.environ['PROBE_STATE'], 'w').write(json.dumps(event))
        print(json.dumps({'event_id':event['id'], 'accepted':True}))
    else:
        event = json.load(open(os.environ['PROBE_STATE']))
        if os.environ.get('CORRUPT_PROBE'):
            event['content'] = 'wrong'
        print(json.dumps([event]))
    sys.exit(0)
if os.environ.get('FAIL_COMMAND') and os.environ['FAIL_COMMAND'] in sys.argv:
    print('secret-stderr-NEVER-PUBLISH', file=sys.stderr)
    sys.exit(1)
if 'tar' in sys.argv and '-cf' in sys.argv:
    import io, tarfile
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode='w') as tar:
        if '/restore/media' in sys.argv:
            entry = tarfile.TarInfo('./buzz-media/object')
            value = b'wrong' if os.environ.get('CORRUPT_RESTORED_ASSET') else b'asset'
            entry.size = len(value)
            tar.addfile(entry, io.BytesIO(value))
    sys.stdout.buffer.write(buf.getvalue())
elif 'pg_dump' in sys.argv:
    sys.stdout.buffer.write(b'PGDMP-private-fixture')
elif 'psql' in sys.argv:
    if ('-Atf' in sys.argv or 'INSERT INTO' in sys.argv[-1]) and '-q' not in sys.argv:
        print('INSERT 0 1')
    if 'crew_staging_startup_gate' in sys.argv[-1]:
        print('{"crew_staging_startup_gate":' + os.environ.get('UNSAFE_STARTUP_STATE', '0') + '}')
    elif 'pending_reminders' in sys.argv[-1]:
        print(os.environ.get('SOURCE_COUNTS', '{"event_count":1,"db_bytes":1000,"schema_version":"20260909","pending_reminders":0,"ephemeral_channels":0}'))
    elif 'event_ids' in sys.argv[-1]:
        print('{"event_ids":["' + 'f'*64 + '"]}')
    elif 'signed_digest' in sys.argv[-1]:
        print('{"event_count":1,"signed_digest":"fixed"}')
    else:
        print('[{"id":"11111111-1111-1111-1111-111111111111","host":"daily:3000"}]')
elif 'cp' in sys.argv:
    if sys.argv[-1] == '-':
        selected = os.environ['ASSET_TAR']
        if os.environ.get('CHANGED_ASSET_TAR'):
            lines = open(os.environ['CALL_LOG']).readlines()
            if sum('"cp"' in line for line in lines) > 1:
                selected = os.environ['CHANGED_ASSET_TAR']
        sys.stdout.buffer.write(open(selected, 'rb').read())
elif 'start' in sys.argv or 'stop' in sys.argv:
    file = os.environ.get('INSPECT_RESULT')
    if file:
        items = json.load(open(file))
        for item in items:
            if item['Id'] in sys.argv:
                item['State']['Running'] = 'start' in sys.argv
                if 'start' in sys.argv:
                    for network in item['NetworkSettings']['Networks'].values():
                        network['NetworkID'] = 'a' * 64
                if os.environ.get('RELAY_EXITS') and item['Id'] == '1' * 64:
                    item['State']['Running'] = False
        open(file, 'w').write(json.dumps(items))
elif 'inspect' in sys.argv:
    if os.environ.get('INVALID_READY_INSPECT') and 'container' in sys.argv and sum(len(v) == 64 for v in sys.argv) == 1:
        print('null');sys.exit(0)
    key = 'SOURCE_INSPECT' if 'daily-postgres' in sys.argv else 'VOLUME_INSPECT' if 'volume' in sys.argv else 'NETWORK_INSPECT' if 'network' in sys.argv else 'INSPECT_RESULT'
    print(open(os.environ[key]).read() if os.environ.get(key) else '[]')
elif 'compose' in sys.argv:
    if 'config' in sys.argv:
        print(open(os.environ['COMPOSE_RESULT']).read())
    else:
        print('')
elif 'info' in sys.argv:
    print(json.dumps({'Name': os.environ.get('SERVER_NAME', ''), 'DockerRootDir': '/var/lib/docker'}))
else:
    print('[]')
"""
        for name in ("docker", "ssh", "pg_dump", "pg_restore", "ss", "buzz", "curl"):
            path = self.bin / name
            path.write_text(fake)
            path.chmod(0o700)
        self.config = {
            "version": 1,
            "environment_id": "crew-staging-test",
            "server_host": "staging-server",
            "source": {
                "host": "staging-server", "relay": "ws://127.0.0.1:3000",
                "database": "buzz", "postgres_container": "daily-postgres",
                "container_ids": {"daily-postgres": "d" * 64, "daily-media": "e" * 64},
                "volumes": ["daily-postgres-data", "daily-media"],
                "paths": [str(self.root / "daily")],
            },
            "target": {
                "relay": "ws://100.86.143.13:3348", "bind_ip": "100.86.143.13",
                "ports": {"relay": 3348, "health": 8348, "metrics": 9348, "postgres": 15348},
                "database": "crew_staging_test", "network": "crew-staging-test-net",
                "volumes": {k: "crew-staging-test-" + k for k in ("postgres", "redis", "media", "git")},
                "root": str(self.root / "target"),
            },
            "snapshot_root": str(self.root / "baselines"),
            "mac": {"source_roots": [str(self.root / "installed")],
                    "app_data": str(self.root / "copy/app"),
                    "profiles": str(self.root / "copy/profiles"),
                    "workspaces": str(self.root / "copy/workspaces")},
            "limits": {"timeout_seconds": 10, "output_bytes": 65536, "cpus": 1,
                       "memory_mb": 512, "snapshot_bytes": 10000000},
            "auth": {"mode": "real", "admin_pubkey": "a" * 64,
                     "secret_refs": {"postgres": "/private/staging/postgres"}},
            "build_sha": "b" * 40, "schema_version": "20260909",
            "images": {k: k + "@sha256:" + "c" * 64 for k in ("relay", "postgres", "redis", "media")},
        }
        self.compose = {"services": {}, "volumes": {}, "networks": {}}
        asset_tar = self.root / 'asset.tar'
        with tarfile.open(asset_tar, 'w') as archive:
            entry = tarfile.TarInfo('buzz-media/object')
            entry.size = 5
            archive.addfile(entry, io.BytesIO(b'asset'))
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                        CALL_LOG=str(self.log), COMPOSE_RESULT=str(self.root / "compose.json"),
                        ASSET_TAR=str(asset_tar))
        source_inspect = self.root / 'source-inspect.json'
        source_inspect.write_text(json.dumps([
            {'Id': identifier, 'Name': '/' + name,
             'Mounts': [{'Type': 'volume', 'Name': volume, 'Source': '/daily/' + volume}]}
            for (name, identifier), volume in zip(self.config['source']['container_ids'].items(), self.config['source']['volumes'])]))
        self.env['SOURCE_INSPECT'] = str(source_inspect)

    def run_cli(self, operation="plan", baseline=None):
        path = self.root / "config.json"
        path.write_text(json.dumps(self.config))
        (self.root / "compose.json").write_text(json.dumps(self.compose))
        args = [sys.executable, str(CLI), operation, "--config", str(path)]
        if baseline:
            args.extend(['--baseline', baseline])
        return subprocess.run(args,
                              env=self.env, text=True, capture_output=True, timeout=10)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def test_mac_plan_is_explicitly_unresolved_and_read_only(self):
        result = self.run_cli()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["status"], "UNRESOLVED")
        self.assertEqual(self.calls(), [])

    def test_source_target_url_alias_refused_before_commands(self):
        self.config["target"]["relay"] = "http://localhost:3000/"
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("source", result.stderr)
        self.assertEqual(self.calls(), [])

    def test_duplicate_ports_refused(self):
        self.config["target"]["ports"]["health"] = 3348
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ports", result.stderr)

    def test_shared_volume_refused_even_with_new_project(self):
        self.config["target"]["volumes"]["postgres"] = "daily-postgres-data"
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("volume", result.stderr)
        self.assertEqual(self.calls(), [])

    def test_zero_limits_refused(self):
        for key in self.config["limits"]:
            with self.subTest(key=key):
                original = self.config["limits"][key]
                self.config["limits"][key] = 0
                result = self.run_cli()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("limit", result.stderr)
                self.config["limits"][key] = original

    def test_symlink_into_daily_refused(self):
        daily = self.root / "daily"
        daily.mkdir()
        (self.root / "target").symlink_to(daily, target_is_directory=True)
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("path", result.stderr)

    def test_baseline_inside_checkout_refused(self):
        self.config["snapshot_root"] = str(ROOT / "private-baselines")
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("checkout", result.stderr)

    def test_embedded_credentials_never_echoed(self):
        self.config["source"]["relay"] = "http://admin:DO-NOT-PRINT@localhost:3000"
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("DO-NOT-PRINT", result.stdout + result.stderr)

    def test_unresolved_restore_never_writes(self):
        result = self.run_cli("restore")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), [])

    def server_config(self):
        self.config['server_host'] = socket.gethostname()
        self.config['source']['host'] = socket.gethostname()
        self.env['SERVER_NAME'] = socket.gethostname()
        environment = self.config['environment_id']
        labels = {'com.nuncio.crew.staging': environment}
        self.compose = {
            'name': environment,
            'services': {
                name: {'image': self.config['images'][name], 'labels': labels,
                       'cpus': 1, 'mem_limit': 536870912,
                       'networks': {environment + '-net': None},
                       'volumes': [{'type': 'volume', 'source': key, 'target': destination}
                                   for key, destination in {
                                       'postgres': {'postgres': '/var/lib/postgresql/data', 'media': '/restore/media', 'git': '/restore/git'},
                                       'redis': {'redis': '/data'}, 'media': {'media': '/data'},
                                       'relay': {'git': '/srv/git'}}[name].items()],
                       'ports': ([{'host_ip': '100.86.143.13', 'published': '3348', 'target': 3000},
                                  {'host_ip': '127.0.0.1', 'published': '8348', 'target': 8080},
                                  {'host_ip': '127.0.0.1', 'published': '9348', 'target': 9102}]
                                 if name == 'relay' else [{'host_ip': '127.0.0.1', 'published': '15348', 'target': 5432}]
                                 if name == 'postgres' else [])}
                for name in ('relay', 'postgres', 'redis', 'media')},
            'volumes': {k: {'name': v, 'labels': labels} for k, v in self.config['target']['volumes'].items()},
            'networks': {environment + '-net': {'name': environment + '-net', 'labels': labels}},
        }

    def test_server_plan_checks_resolved_resources_without_mutation(self):
        self.server_config()
        result = self.run_cli()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['status'], 'PLANNED')
        self.assertGreater(len(self.calls()), 0)
        self.assertTrue(all(call[0] == 'docker' for call in self.calls()))
        self.assertFalse(any(word in call for call in self.calls() for word in ('up', 'exec', 'stop', 'rm')))

    def test_compose_missing_required_volume_refused(self):
        self.server_config()
        self.compose['services']['media']['volumes'] = []
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('mount inventory', result.stderr)

    def test_compose_shared_actual_volume_refused(self):
        self.server_config()
        self.compose['volumes']['postgres']['name'] = 'daily-postgres-data'
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('resolved volume', result.stderr)

    def test_compose_unowned_bind_mount_refused(self):
        self.server_config()
        self.compose['services']['postgres']['volumes'] = [
            {'type': 'bind', 'source': str(self.root / 'daily'), 'target': '/data'}]
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('bind mount', result.stderr)

    def test_compose_admin_port_public_refused(self):
        self.server_config()
        self.compose['services']['relay']['ports'][1]['host_ip'] = '0.0.0.0'
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('binding', result.stderr)

    def test_compose_external_network_refused(self):
        self.server_config()
        self.compose['networks']['crew-staging-test-net']['external'] = True
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('network', result.stderr)

    def test_compose_readonly_ancestor_mount_refused(self):
        self.server_config()
        self.compose['services']['postgres']['volumes'].append(
            {'type': 'bind', 'source': str(self.root), 'target': '/run/staging/postgres-password', 'read_only': True})
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('bind mount', result.stderr)

    def test_postgres_fixture_port_cannot_be_public(self):
        self.server_config()
        self.compose['services']['postgres']['ports'][0]['host_ip'] = '0.0.0.0'
        result = self.run_cli()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('binding', result.stderr)


class SnapshotTests(unittest.TestCase):
    run_cli = PlanTests.run_cli
    calls = PlanTests.calls
    server_config = PlanTests.server_config

    def setUp(self):
        PlanTests.setUp(self)
        self.server_config()
        self.git = self.root / 'daily' / 'repos'
        self.git.mkdir(parents=True)
        self.config['source'].update(postgres_user='buzz', schema_version='20260909',
                                     community_id='11111111-1111-1111-1111-111111111111',
                                     paths=[str(self.git)],
                                     assets={'git': {'path': str(self.git)},
                                             'media': {'container': 'daily-media', 'path': '/data/buzz-media'}})

    def test_snapshot_records_complete_protected_baseline_and_exclusions(self):
        result = self.run_cli('snapshot', 'fixed')
        self.assertEqual(result.returncode, 0, result.stderr)
        folder = self.root / 'baselines/fixed'
        state = json.loads((folder / 'manifest.json').read_text())
        self.assertEqual(state['status'], 'COMPLETE')
        self.assertEqual(set(state['checksums']), {'database.dump', 'communities.json', 'users.json', 'media.tar', 'git.tar'})
        self.assertEqual((folder / 'database.dump').stat().st_mode & 0o777, 0o600)
        dump = next(call for call in self.calls() if 'pg_dump' in call)
        for table in ('communities', 'api_tokens', 'workflows', 'push_leases', 'subscriptions'):
            self.assertIn('--exclude-table-data=public.' + table, dump)
        self.assertFalse(any('UPDATE' in arg or 'DELETE' in arg for call in self.calls() for arg in call))

    def test_duplicate_baseline_never_refreshes(self):
        self.assertEqual(self.run_cli('snapshot', 'fixed').returncode, 0)
        before = self.calls()
        result = self.run_cli('snapshot', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('already exists', result.stderr)
        self.assertEqual(before, self.calls())

    def test_interrupted_dump_leaves_incomplete_and_redacts_stderr(self):
        self.env['FAIL_COMMAND'] = 'pg_dump'
        result = self.run_cli('snapshot', 'interrupted')
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('secret-stderr-NEVER-PUBLISH', result.stdout + result.stderr)
        manifest = self.root / 'baselines/interrupted/manifest.json'
        self.assertTrue(manifest.exists(), 'failed snapshot must leave an incomplete record')
        state = json.loads(manifest.read_text())
        self.assertEqual(state['status'], 'INCOMPLETE')

    def test_missing_git_assets_does_not_create_complete_baseline(self):
        self.git.rmdir()
        result = self.run_cli('snapshot', 'missing')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('asset', result.stderr)

    def test_schema_mismatch_fails_before_capture(self):
        self.config['source']['schema_version'] = 'old'
        result = self.run_cli('snapshot', 'old')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('schema', result.stderr)
        self.assertEqual(self.calls(), [])

    def test_archive_link_refused(self):
        with tarfile.open(self.env['ASSET_TAR'], 'w') as archive:
            item = tarfile.TarInfo('buzz-media/escape')
            item.type = tarfile.SYMTYPE
            item.linkname = '/daily'
            archive.addfile(item)
        result = self.run_cli('snapshot', 'link')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('symlink', result.stderr)

    def test_assets_changing_during_dump_leave_incomplete(self):
        changed = self.root / 'changed.tar'
        with tarfile.open(changed, 'w') as archive:
            item = tarfile.TarInfo('buzz-media/object')
            item.size = 7
            archive.addfile(item, io.BytesIO(b'changed'))
        self.env['CHANGED_ASSET_TAR'] = str(changed)
        result = self.run_cli('snapshot', 'racy')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('capture changed', result.stderr)
        state = json.loads((self.root / 'baselines/racy/manifest.json').read_text())
        self.assertEqual(state['status'], 'INCOMPLETE')

    def test_snapshot_records_signed_event_fingerprint(self):
        result = self.run_cli('snapshot', 'events')
        self.assertEqual(result.returncode, 0, result.stderr)
        state = json.loads((self.root / 'baselines/events/manifest.json').read_text())
        self.assertEqual(state.get('events'), {'event_count': 1, 'signed_digest': 'fixed'})

    def test_local_asset_write_is_bounded_before_archive_exceeds_cap(self):
        self.config['limits']['snapshot_bytes'] = 2048
        (self.git / 'large').write_bytes(b'x' * 4096)
        result = self.run_cli('snapshot', 'large')
        self.assertNotEqual(result.returncode, 0)
        archives = list((self.root / 'baselines/large').glob('*.tar'))
        self.assertTrue(all(file.stat().st_size <= 2048 for file in archives),
                        'failed capture must not first write an oversized tar')

    def test_pending_signed_reminders_block_snapshot(self):
        self.env['SOURCE_COUNTS'] = '{"event_count":1,"db_bytes":1000,"pending_reminders":1,"ephemeral_channels":0}'
        result = self.run_cli('snapshot', 'scheduled')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('scheduled', result.stderr)
        self.assertFalse(any('pg_dump' in call for call in self.calls()))

    def test_changed_source_container_blocks_capture(self):
        items = json.loads(Path(self.env['SOURCE_INSPECT']).read_text())
        items[0]['Id'] = 'f' * 64
        Path(self.env['SOURCE_INSPECT']).write_text(json.dumps(items))
        result = self.run_cli('snapshot', 'changed-source')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('source identity', result.stderr)
        self.assertFalse(any('pg_dump' in call for call in self.calls()))


class RestoreTests(unittest.TestCase):
    run_cli = PlanTests.run_cli
    calls = PlanTests.calls
    server_config = PlanTests.server_config

    def setUp(self):
        SnapshotTests.setUp(self)
        self.assertEqual(self.run_cli('snapshot', 'fixed').returncode, 0)

    def test_corrupted_baseline_refused_before_target_commands(self):
        (self.root / 'baselines/fixed/database.dump').write_bytes(b'corrupt')
        before = self.calls()
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('checksum', result.stderr)
        self.assertEqual(before, self.calls())

    def test_incomplete_baseline_refused_before_target_commands(self):
        file = self.root / 'baselines/fixed/manifest.json'
        manifest = json.loads(file.read_text())
        manifest['status'] = 'INCOMPLETE'
        file.write_text(json.dumps(manifest))
        before = self.calls()
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('incomplete', result.stderr)
        self.assertEqual(before, self.calls())

    def test_changed_target_refused_before_target_commands(self):
        self.config['target']['database'] = 'crew_staging_test_other'
        before = self.calls()
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('target/config', result.stderr)
        self.assertEqual(before, self.calls())

    def test_no_owned_resources_cannot_restore_or_start_relay(self):
        result = self.run_cli('restore', 'fixed')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('ownership', result.stderr)
        self.assertFalse(any('pg_restore' in call or 'up' in call for call in self.calls()))


if __name__ == "__main__":
    unittest.main()
