"""Apply only explicitly hashed, contiguous branch migrations to owned staging."""

import hashlib
from pathlib import Path

from crew_staging_config import require
from crew_staging_process import run


def migration_plan(config):
    source, target = config['source'].get('schema_version'), config['schema_version']
    entries = config.get('migrations', [])
    if source == target:
        require(not entries, 'unexpected migration sequence for matching schema')
        return []
    require(entries and str(source).isdigit() and str(target).isdigit(), 'incompatible source schema; recorded migration required')
    require([entry.get('version') for entry in entries] == list(range(int(source) + 1, int(target) + 1)),
            'incomplete migration sequence')
    root = Path(__file__).resolve().parents[1] / 'migrations'
    result = []
    for entry in entries:
        file = Path(entry['path']).resolve()
        require(file.parent == root and file.name.startswith(f"{entry['version']:04d}_") and file.suffix == '.sql',
                'migration must be the recorded branch file')
        content = file.read_bytes()
        require(hashlib.sha256(content).hexdigest() == entry['sha256'], 'migration checksum mismatch')
        result.append((entry['version'], file, content))
    return result


def apply_migrations(config, container, directory):
    for version, source, content in migration_plan(config):
        description = source.stem.split('_', 1)[1].replace('_', ' ').replace("'", "''")
        checksum = hashlib.sha384(content).hexdigest()
        ledger = ("\nINSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time) VALUES ("
                  + str(version) + ",'" + description + "',true,decode('" + checksum + "','hex'),0);\n")
        file = directory / ('migration-' + str(version) + '.sql')
        file.write_bytes(content + ledger.encode())
        file.chmod(0o600)
        prefix = ['docker', '--host', 'unix:///var/run/docker.sock']
        run(prefix + ['cp', str(file), container + ':/tmp/crew-staging-migration.sql'], config['limits'])
        run(prefix + ['exec', container, 'timeout', '-s', 'KILL',
                      str(max(1, int(config['limits']['timeout_seconds']) - 1)),
                      'psql', '-X', '-v', 'ON_ERROR_STOP=1', '--single-transaction',
                      '-U', 'staging', '-d', config['target']['database'], '-f', '/tmp/crew-staging-migration.sql'], config['limits'])
