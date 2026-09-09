"""Read-only source identity, load budget and executable-state gates."""

from crew_staging_config import overlaps, require
from crew_staging_process import run_json


COUNTS_SQL = """SELECT json_build_object(
'event_count',(SELECT count(*) FROM events),
'db_bytes',pg_database_size(current_database()),
'schema_version',(SELECT max(version)::text FROM _sqlx_migrations),
'pending_reminders',(SELECT count(*) FROM events WHERE kind=30300 AND not_before IS NOT NULL
AND delivered_at IS NULL AND deleted_at IS NULL),
'ephemeral_channels',(SELECT count(*) FROM channels WHERE ttl_deadline IS NOT NULL AND archived_at IS NULL))"""


def source_command(config, *args):
    """Bound the database process inside Docker, even if its client is killed."""
    seconds = max(1, int(config['limits']['timeout_seconds']) - 1)
    return ['docker', '--host', 'unix:///var/run/docker.sock', 'exec', '-e',
            'PGOPTIONS=-c default_transaction_read_only=on -c statement_timeout=' + str(seconds * 1000),
            config['source']['postgres_container'], 'timeout', '-s', 'KILL', str(seconds), *args]


def inspect_source(config):
    source = config['source']
    ids = source.get('container_ids', {})
    require(ids and source['postgres_container'] in ids, 'recorded source identity required')
    require(all(asset.get('container') in ids for asset in source.get('assets', {}).values()
                if 'container' in asset), 'every asset source requires a recorded container identity')
    items = run_json(['docker', '--host', 'unix:///var/run/docker.sock', 'container', 'inspect', *ids], config)
    require(isinstance(items, list) and len(items) == len(ids), 'source identity inventory mismatch')
    seen = set()
    for item in items:
        name = item.get('Name', '').lstrip('/')
        require(ids.get(name) == item.get('Id') and name not in seen, 'source identity changed')
        seen.add(name)
        for mount in item.get('Mounts', []):
            require(mount.get('Type') == 'volume' and mount.get('Name') in source['volumes'],
                    'source storage inventory changed')
            require(mount['Name'] not in config['target']['volumes'].values()
                    and not overlaps(mount['Source'], config['target']['root']), 'source storage overlaps target')
    result = run_json(source_command(config, 'psql', '-X', '-v', 'ON_ERROR_STOP=1',
                                    '-U', source['postgres_user'], '-d', source['database'],
                                    '-Atc', COUNTS_SQL), config)
    require(isinstance(result, dict) and type(result.get('event_count')) is int
            and 0 <= result['event_count'] <= min(10000, max(0, (config['limits']['output_bytes'] - 1024) // 70)), 'source event-count budget exceeded')
    require(type(result.get('db_bytes')) is int and 0 < result['db_bytes'] <= config['limits']['snapshot_bytes'],
            'source database capture budget exceeded')
    require(result.get('pending_reminders') == 0 and result.get('ephemeral_channels') == 0,
            'scheduled source state requires separate policy; snapshot/start refused')
    require(result.get('schema_version') == source['schema_version'], 'actual source schema mismatch')
    return result
