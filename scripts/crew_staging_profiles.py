"""Copy bounded non-secret preferences by value, never managed employee state."""

import hashlib
import json
import os
import re
import socket
from pathlib import Path

from crew_staging_baseline import write_json
from crew_staging_config import path, require


FILES = ('managed-agents.json', 'global-agent-config.json')


def load_ownership(file, expected_sha256):
    """Load the operator-reviewed digest; this never grants runtime readiness."""
    file = Path(file)
    require(not file.is_symlink() and file.stat().st_size <= 65536
            and file.stat().st_uid == os.getuid() and file.stat().st_mode & 0o077 == 0,
            'private owned ownership manifest required')
    data = file.read_bytes()
    require(len(data) <= 65536 and hashlib.sha256(data).hexdigest() == expected_sha256,
            'ownership manifest digest mismatch')
    envelope = json.loads(data)
    require(envelope['schema'] == 'crew-staging-ownership' and envelope['version'] == 1,
            'unsupported ownership schema')
    mac = envelope['mac']
    require(mac['host'] == socket.gethostname() and mac['owner_uid'] == os.getuid()
            and mac['home'] == str(Path.home().resolve()), 'ownership host/user mismatch')
    home = Path(mac['home'])
    environment = envelope['environment_id']
    require(re.fullmatch(r'crew-staging-[a-z0-9-]{1,40}', environment), 'owned environment name required')
    slug = environment.removeprefix('crew-')
    bundle = 'com.nuncio.crew.' + slug
    expected = {'app_data': home / 'Library/Application Support' / bundle,
                'config_home': home / 'Library/Application Support' / ('buzz-demo-' + slug),
                'nest': home / ('.buzz-demo-' + slug),
                'profiles': home / environment / 'profiles', 'workspaces': home / environment / 'workspaces'}
    require(mac['roots'] == {key: str(value) for key, value in expected.items()}
            and all(value.resolve() == value for value in expected.values()), 'ownership root alias or namespace mismatch')
    require(mac['build_demo_slug'] == slug and mac['bundle_id'] == bundle
            and mac['keyring_service'] == 'buzz-desktop-demo.' + slug
            and mac['deep_link_scheme'] == 'buzz-demo-' + slug, 'build identity namespace mismatch')
    source = home / 'Library/Application Support/com.nuncio.crew/agents'
    require(mac['profile_source'] == str(source) and source.resolve() == source
            and mac['auth_references'] == [] and mac['runtime_generation_allowed'] is False,
            'profile-only source/auth boundary required')
    return {'environment_id': environment, 'mac': {'host': mac['host'], 'profile_source': str(source),
            'source_roots': [str(source.parent)], 'profiles': str(expected['profiles'])}}


def read_source(root):
    result = {}
    for name in FILES:
        require(not (root / name).is_symlink(), 'profile source symlink refused')
        file = path(str(root / name))
        require(file.is_file() and file.stat().st_size <= 1048576
                and file.stat().st_uid == os.getuid(), 'profile source must be bounded and owned')
        with file.open('rb') as stream:
            payload = stream.read(1048577)
        require(len(payload) <= 1048576, 'profile source grew beyond budget')
        result[name] = payload
    return result


def choice(value):
    if value == '':
        return None
    require(value is None or isinstance(value, str) and len(value) <= 200
            and re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9._:/ -]*(?:\[[a-zA-Z0-9_.,= -]+\])?', value),
            'profile choice must be a bounded identifier')
    return value


def export_profiles(config):
    mac = config['mac']
    require(mac.get('host') == socket.gethostname(), 'profile export must run on the named Mac')
    source = path(mac['profile_source'])
    require(any(source == path(root) or path(root) in source.parents for root in mac['source_roots']),
            'profile source outside declared installed roots')
    destination = path(mac['profiles'])
    require(not destination.exists(), 'profile destination exists; no implicit refresh')
    before = read_source(source)
    agents = json.loads(before[FILES[0]])
    global_config = json.loads(before[FILES[1]])
    require(isinstance(agents, list) and len(agents) <= 1000 and isinstance(global_config, dict),
            'invalid source profile structure')
    choices = []
    for agent in agents:
        require(isinstance(agent, dict), 'invalid source profile entry')
        item = {key: choice(agent.get(original)) for key, original in
                (('runtime', 'agent_command'), ('model', 'model'), ('provider', 'provider'))}
        require(item['runtime'] is None or re.fullmatch(r'[a-zA-Z0-9._-]{1,100}', item['runtime']),
                'runtime must be a catalog command name, not a path or command line')
        if item not in choices:
            choices.append(item)
    preferences = {key: choice(global_config.get(key)) for key in ('model', 'provider', 'preferred_runtime')}
    require(before == read_source(source), 'source profiles changed during capture; retry')
    destination.mkdir(parents=True, mode=0o700)
    os.chmod(destination, 0o700)
    receipt = {'status': 'PROFILES_EXPORTED_NOT_LAUNCHED', 'environment_id': config['environment_id'],
               'runtime_choices': choices, 'global_preferences': preferences, 'auth_references': [],
               'source_sha256': {name: hashlib.sha256(payload).hexdigest() for name, payload in before.items()},
               'source_unchanged': True, 'credential_isolation': 'NO_CREDENTIALS_IMPORTED',
               'source_comparison': 'equal across capture and pre-write readback; not an ongoing source lock',
               'desktop_status': 'NOT_LAUNCHED'}
    require(before == read_source(source), 'source profiles changed before export; no success receipt written')
    write_json(destination / 'profile-import.json', receipt)
    return {'status': receipt['status'], 'path': str(destination / 'profile-import.json'),
            'source_unchanged': True, 'credential_isolation': receipt['credential_isolation']}


if __name__ == '__main__':
    import argparse
    from crew_staging_config import Refused
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ownership', required=True)
    parser.add_argument('--sha256', required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(export_profiles(load_ownership(args.ownership, args.sha256))))
    except (Refused, OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(2, (str(error) if isinstance(error, Refused) else 'invalid profile ownership manifest') + '\n')
