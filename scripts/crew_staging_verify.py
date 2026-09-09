"""A server receipt requires isolated destinations and exact probe readback."""

from datetime import datetime, timezone
import hashlib
import json
import os
import re
import uuid

from crew_staging_baseline import event_subset_sql, load_baseline, source_sql, write_json
from crew_staging_config import path, require
from crew_staging_lock import operation_lock
from crew_staging_process import run_json
from crew_staging_restore import DOCKER, owned_resources, target_sql, wait_relay


def baseline_event_batches(manifest):
    ids = manifest.get('event_ids')
    require(isinstance(ids, list) and all(re.fullmatch(r'[a-f0-9]{64}', value) for value in ids),
            'baseline event IDs missing')
    batches = manifest.get('event_batches')
    require(isinstance(batches, list) and 1 <= len(batches) <= 200
            and {value for batch in batches for value in batch['ids']} == set(ids),
            'baseline event batch inventory mismatch')
    return [(event_subset_sql(batch['ids']), batch['events']) for batch in batches]


def probe_environment(config):
    probe = config['probe']
    cli = path(probe['cli'])
    require(cli.is_file() and hashlib.sha256(cli.read_bytes()).hexdigest() == probe['cli_sha256'],
            'probe executable changed')
    key_file = path(probe['admin_key_file'])
    require(path(config['target']['root']) in key_file.parents and key_file.is_file()
            and key_file.stat().st_uid == os.getuid() and key_file.stat().st_mode & 0o077 == 0
            and key_file.stat().st_nlink == 1, 'probe credential must be separately owned private staging file')
    key = key_file.read_text().strip()
    require(re.fullmatch(r'[a-f0-9]{64}', key), 'invalid staging signer reference')
    env = {name: value for name, value in os.environ.items()
           if not name.startswith('BUZZ_') and not name.upper().endswith('_PROXY')}
    env.update(BUZZ_PRIVATE_KEY=key, BUZZ_RELAY_URL=config['target']['relay'], RELAY_URL=config['target']['relay'])
    return str(cli), env


def verify(config, baseline):
    _, manifest = load_baseline(config, baseline)
    root = path(config['target']['root'])
    with operation_lock(root):
        state = json.loads((root / 'state.json').read_text())
        require(state.get('baseline_id') == baseline and state.get('status') in ('RESTORED_UNVERIFIED', 'VERIFYING', 'SERVER_VERIFIED'),
                'staging unavailable or verification incomplete; restore the fixed baseline first')
        ids = owned_resources(config)
        inspected = run_json(DOCKER + ['container', 'inspect', *ids.values()], config)
        require(all(item.get('State', {}).get('Running') for item in inspected), 'staging services are not running')
        batches = baseline_event_batches(manifest)
        for query, expected in batches:
            require(target_sql(config, ids, query) == expected, 'baseline signed event readback changed')
            require(source_sql(config, query) == expected, 'daily signed event sentinel changed')
        wait_relay(config, ids['relay'])
        cli, env = probe_environment(config)
        state['status'] = 'VERIFYING'
        write_json(root / 'state.json', state)
        channel = state.get('probe_channel_id')
        if not channel:
            created = run_json([cli, 'channels', 'create', '--name', config['environment_id'] + '-verify',
                                '--type', 'stream', '--visibility', 'private'], config, env)
            channel = str(uuid.UUID(created['channel_id']))
            state['probe_channel_id'] = channel
            write_json(root / 'state.json', state)
        event_id = state.get('probe_event_id')
        content = state.get('probe_content')
        if not event_id:
            content = content or 'Staging verification ' + str(uuid.uuid4())
            state['probe_content'] = content
            write_json(root / 'state.json', state)
            sent = run_json([cli, 'messages', 'send', '--channel', channel, '--content', content], config, env)
            event_id = sent.get('event_id')
            require(sent.get('accepted') is True and isinstance(event_id, str) and re.fullmatch(r'[a-f0-9]{64}', event_id),
                    'staging probe publication not accepted')
            state['probe_event_id'] = event_id
            write_json(root / 'state.json', state)
        require(isinstance(content, str) and isinstance(event_id, str) and re.fullmatch(r'[a-f0-9]{64}', event_id),
                'persisted staging probe identity missing')
        events = run_json([cli, 'messages', 'get', '--channel', channel, '--limit', '5'], config, env)
        require(isinstance(events, list) and any(event.get('id') == event_id and event.get('content') == content
                and event.get('pubkey') == config['auth']['admin_pubkey'] for event in events),
                'staging probe persisted readback mismatch')
        for query, expected in batches:
            require(source_sql(config, query) == expected, 'daily signed event sentinel changed during probe')
        state.update(status='SERVER_VERIFIED', auth_class=config['auth']['mode'],
                     verified_at=datetime.now(timezone.utc).isoformat(), build_sha=config['build_sha'],
                     relay=config['target']['relay'], desktop_status='NOT_VERIFIED_ON_THIS_HOST')
        write_json(root / 'state.json', state)
        return state
