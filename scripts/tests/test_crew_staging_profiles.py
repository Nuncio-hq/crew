"""Profile export uses the production CLI and never copies employee state."""

import json
import unittest

import test_crew_staging as fixture


class ProfileTests(unittest.TestCase):
    run_cli = fixture.PlanTests.run_cli

    def setUp(self):
        fixture.PlanTests.setUp(self)
        self.source = self.root / 'installed/agents'
        self.source.mkdir(parents=True)
        self.config['mac']['source_roots'] = [str(self.source.parent)]
        self.config['mac']['profile_source'] = str(self.source)
        self.config['mac']['host'] = __import__('socket').gethostname()
        (self.source / 'managed-agents.json').write_text(json.dumps([{
            'provider': 'openai-codex', 'model': 'model-a', 'agent_command': 'codex-acp',
            'private_key_nsec': 'PRIVATE', 'auth_tag': 'TOKEN', 'env_vars': {'KEY': 'SECRET'},
            'name': 'employee', 'relay_url': 'ws://daily', 'system_prompt': 'PRIVATE PROMPT',
            'agent_args': ['--token', 'SECRET'], 'start_on_app_launch': True,
        }]))
        (self.source / 'global-agent-config.json').write_text(json.dumps({
            'model': 'model-a', 'provider': 'openai-codex', 'preferred_runtime': 'codex-acp',
            'env_vars': {'TOKEN': 'SECRET'}, 'unknown': 'PRIVATE',
        }))

    def test_export_is_inert_allowlist_with_source_readback(self):
        before = {p.name: p.read_bytes() for p in self.source.iterdir()}
        result = self.run_cli('profiles')
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt['status'], 'PROFILES_EXPORTED_NOT_LAUNCHED')
        output = self.root / 'copy/profiles'
        envelope = json.loads((output / 'profile-import.json').read_text())
        self.assertEqual(envelope['runtime_choices'], [{'runtime': 'codex-acp', 'provider': 'openai-codex', 'model': 'model-a'}])
        self.assertEqual(envelope['auth_references'], [])
        self.assertEqual(before, {p.name: p.read_bytes() for p in self.source.iterdir()})
        combined = ''.join(p.read_text() for p in output.iterdir())
        for forbidden in ('PRIVATE', 'SECRET', 'employee', 'ws://daily', 'TOKEN'):
            self.assertNotIn(forbidden, combined)
        self.assertFalse((output / 'managed-agents.json').exists())
        self.assertEqual((output / 'profile-import.json').stat().st_mode & 0o777, 0o600)

    def test_existing_destination_is_not_refreshed(self):
        self.assertEqual(self.run_cli('profiles').returncode, 0)
        self.assertNotEqual(self.run_cli('profiles').returncode, 0)

    def test_legacy_empty_runtime_and_model_annotations_are_inert_choices(self):
        file = self.source / 'managed-agents.json'
        agents = json.loads(file.read_text())
        agents[0].update(agent_command='', model='model-a[reasoning=high,context=1m]')
        file.write_text(json.dumps(agents))
        result = self.run_cli('profiles')
        self.assertEqual(result.returncode, 0, result.stderr)
        envelope = json.loads((self.root / 'copy/profiles/profile-import.json').read_text())
        self.assertEqual(envelope['runtime_choices'][0]['runtime'], None)
        self.assertEqual(envelope['runtime_choices'][0]['model'], agents[0]['model'])

    def test_symlink_source_refused_without_copy(self):
        file = self.source / 'managed-agents.json'
        file.rename(self.source / 'original.json')
        file.symlink_to(self.source / 'original.json')
        result = self.run_cli('profiles')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / 'copy/profiles').exists())


if __name__ == '__main__':
    unittest.main()
