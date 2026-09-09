"""Restore only pre-provisioned, explicitly recorded staging resource IDs."""

import json
import os
import re
import time
import tempfile
from pathlib import Path
from urllib.parse import urlsplit

from crew_staging_baseline import EVENTS_SQL, EXCLUDED, archive_inventory, load_baseline, write_json
from crew_staging_config import Refused, on_server, path, require
from crew_staging_process import run, run_json
from crew_staging_resources import LABEL, VOLUME_DESTINATIONS, inspect_plan
from crew_staging_migrations import migration_plan, apply_migrations
from crew_staging_lock import operation_lock


DOCKER = ["docker", "--host", "unix:///var/run/docker.sock"]


def expected_bindings(config, name):
    target = config['target']
    if name == 'postgres':
        return {'5432/tcp': [{'HostIp': '127.0.0.1', 'HostPort': str(target['ports']['postgres'])}]}
    if name == 'relay':
        return {str(port) + '/tcp': [{'HostIp': host, 'HostPort': str(target['ports'][key])}]
                for key, host, port in (('relay', target['bind_ip'], 3000),
                                       ('health', '127.0.0.1', 8080), ('metrics', '127.0.0.1', 9102))}
    return {}


def owned_resources(config):
    require(on_server(config), "ownership verification must run on the named server")
    ids = config["target"].get("container_ids", {})
    require(set(ids) == {"postgres", "redis", "media", "relay"}
            and all(re.fullmatch(r"[a-f0-9]{64}", value) for value in ids.values())
            and len(set(ids.values())) == 4, "recorded resource ownership IDs required")
    inspected = run_json(DOCKER + ["container", "inspect", *ids.values()], config)
    require(isinstance(inspected, list) and len(inspected) == 4, "resource ownership inventory changed")
    by_id = {item["Id"]: item for item in inspected}
    require(set(by_id) == set(ids.values()), "resource ownership ID changed")
    volume_items = run_json(DOCKER + ["volume", "inspect", *config["target"]["volumes"].values()], config)
    require(isinstance(volume_items, list) and len(volume_items) == 4, "volume ownership inventory changed")
    expected_volumes = {value: key for key, value in config["target"]["volumes"].items()}
    require({volume.get("Name") for volume in volume_items} == set(expected_volumes), "volume ownership names changed")
    for volume in volume_items:
        require(volume.get("Driver") == "local" and not volume.get("Options")
                and volume.get("Labels", {}).get(LABEL) == config["environment_id"]
                and volume.get("Mountpoint") == config["target"].get("volume_mounts", {}).get(expected_volumes[volume["Name"]]),
                "volume driver, mountpoint or ownership changed")
    network_items = run_json(DOCKER + ["network", "inspect", config["target"]["network"]], config)
    require(isinstance(network_items, list) and len(network_items) == 1
            and network_items[0].get("Id") == config["target"].get("network_id")
            and network_items[0].get("Driver") == "bridge"
            and network_items[0].get("Labels", {}).get(LABEL) == config["environment_id"], "network ownership changed")
    for name, identifier in ids.items():
        item = by_id[identifier]
        require(item.get("Config", {}).get("Labels", {}).get(LABEL) == config["environment_id"],
                "resource ownership label changed")
        require(item["Config"].get("Image") == config["images"][name], "running resource image changed")
        host_config = item.get('HostConfig', {})
        require(not host_config.get('Privileged') and not host_config.get('Devices')
                and not host_config.get('CapAdd') and not host_config.get('PidMode')
                and host_config.get('IpcMode', 'private') in ('', 'private'),
                'unexpected container privileges or host namespaces')
        require(item.get("HostConfig", {}).get("NetworkMode") == config["target"]["network"],
                "running resource network changed")
        networks = item.get("NetworkSettings", {}).get("Networks", {})
        require(set(networks) == {config["target"]["network"]}
                and (networks[config["target"]["network"]].get("NetworkID") == config["target"].get("network_id")
                     or not item.get('State', {}).get('Running')
                     and networks[config["target"]["network"]].get("NetworkID") == ""),
                "running resource network identity changed")
        require(item["HostConfig"].get("NanoCpus") == config["limits"]["cpus"] * 1000000000
                and item["HostConfig"].get("Memory") == config["limits"]["memory_mb"] * 1048576,
                "running resource limits changed")
        bindings = expected_bindings(config, name)
        require((item['HostConfig'].get('PortBindings') or {}) == bindings,
                'effective service port bindings changed')
        if item.get('State', {}).get('Running'):
            active = {key: value for key, value in item['NetworkSettings'].get('Ports', {}).items() if value}
            require(active == bindings, 'active service port bindings changed')
        if name == "relay":
            environment = dict(entry.split("=", 1) for entry in item["Config"].get("Env", []) if "=" in entry)
            db = urlsplit(environment.get("DATABASE_URL", ""))
            require(db.hostname == "postgres" and db.port == 5432 and db.username == "staging"
                    and db.path == "/" + config["target"]["database"] and not db.query,
                    "effective database destination mismatch")
            for key, value in {"RELAY_URL": config["target"]["relay"], "REDIS_URL": "redis://redis:6379",
                               "BUZZ_S3_ENDPOINT": "http://media:9000", "BUZZ_REQUIRE_AUTH_TOKEN": "true",
                               "BUZZ_REQUIRE_RELAY_MEMBERSHIP": "true", "BUZZ_AUTO_MIGRATE": "false",
                               "BUZZ_PUSH_ENABLED": "false",
                               "RELAY_OWNER_PUBKEY": config["auth"]["admin_pubkey"]}.items():
                require(environment.get(key) == value, "effective relay configuration mismatch: " + key)
            require(not environment.get("READ_DATABASE_URL") and not environment.get("OTEL_EXPORTER_OTLP_ENDPOINT"),
                    "undeclared external service destination")
        expected_mounts = {(config["target"]["volumes"][key], destination)
                           for key, destination in VOLUME_DESTINATIONS[name].items()}
        mounts = item.get("Mounts", [])
        actual_mounts = {(mount.get("Name"), mount.get("Destination"))
                         for mount in mounts if mount.get("Type") == "volume"}
        require(actual_mounts == expected_mounts
                and len(mounts) == len(expected_mounts) + (1 if name == "postgres" else 0),
                "required service mount inventory changed")
        for mount in mounts:
            volume_key = next((key for key, value in config["target"]["volumes"].items() if value == mount.get("Name")), None)
            require(mount.get("Type") == "volume" and volume_key is not None
                    and mount.get("RW") is True
                    and mount.get("Source") == config["target"].get("volume_mounts", {}).get(volume_key)
                    or mount.get("Type") == "bind" and mount.get("RW") is False
                    and path(mount.get("Source", "")) == path(config["target"]["root"]) / "secrets/postgres-password"
                    and name == "postgres" and mount.get("Destination") == "/run/staging/postgres-password",
                    "running resource has unowned mount")
    return ids


def port_preflight(config, ids):
    inspected = run_json(DOCKER + ["container", "inspect", ids['relay'], ids['postgres']], config)
    owned = set()
    for name in ('relay', 'postgres'):
        item = next((item for item in inspected if item.get('Id') == ids[name]), None)
        require(item is not None, 'resource ownership changed during port inspection')
        bindings = expected_bindings(config, name)
        require(item.get('HostConfig', {}).get('PortBindings') == bindings, 'service port bindings changed')
        if item.get('State', {}).get('Running'):
            active = {key: value for key, value in item.get('NetworkSettings', {}).get('Ports', {}).items() if value}
            require(active == bindings, 'active service port bindings changed')
            owned.update((entry['HostIp'], entry['HostPort']) for entries in bindings.values() for entry in entries)
    listeners = run(["ss", "-H", "-ltn"], config["limits"]).decode()
    for line in listeners.splitlines():
        if not line.strip():
            continue
        fields = line.split()
        require(len(fields) >= 4 and fields[0] == "LISTEN", "unrecognized listener inventory")
        host, port = fields[3].rsplit(":", 1)
        require(port not in {str(value) for value in config["target"]["ports"].values()} or (host, port) in owned,
                "staging port conflict: " + port)


def execute(config, *args):
    argv = list(args)
    if argv[:1] == ["exec"]:
        argv[2:2] = ["timeout", "-s", "KILL", str(int(config["limits"]["timeout_seconds"]))]
    return run(DOCKER + argv, config["limits"])


def target_sql(config, ids, sql):
    if len(sql.encode()) > 32768:
        descriptor, name = tempfile.mkstemp(prefix='target-query-', suffix='.sql', dir=path(config['target']['root']))
        query_file = Path(name)
        with os.fdopen(descriptor, 'w', encoding='utf-8') as stream:
            stream.write(sql)
        execute(config, 'cp', str(query_file), ids['postgres'] + ':/tmp/crew-staging-query.sql')
        result = execute(config, 'exec', ids['postgres'], 'psql', '-X', '-v', 'ON_ERROR_STOP=1',
                         '-q', '-U', 'staging', '-d', config['target']['database'], '-Atf', '/tmp/crew-staging-query.sql')
        execute(config, 'exec', ids['postgres'], 'rm', '/tmp/crew-staging-query.sql')
        query_file.unlink()
        return json.loads(result)
    return run_json(DOCKER + ["exec", ids["postgres"], "timeout", "-s", "KILL",
                             str(max(1, int(config['limits']['timeout_seconds']) - 1)),
                             "psql", "-X", "-q", "-v", "ON_ERROR_STOP=1",
                             "-U", "staging", "-d", config["target"]["database"], "-Atc", sql], config)


def wait_postgres(config, identifier):
    deadline = time.monotonic() + config['limits']['timeout_seconds']
    for _ in range(20):
        try:
            execute(config, 'exec', identifier, 'pg_isready', '-h', '127.0.0.1',
                    '-p', '5432', '-U', 'staging', '-t', '1')
            return
        except Refused:
            require(time.monotonic() + 0.25 < deadline, 'staging PostgreSQL TCP readiness deadline exceeded')
            time.sleep(0.25)
    raise Refused('staging PostgreSQL TCP readiness attempts exhausted')


def wait_relay(config, identifier):
    """Wait within one deadline, refusing an exited relay immediately."""
    deadline = time.monotonic() + config['limits']['timeout_seconds']
    for _ in range(256):
        def bounded_config():
            remaining = deadline - time.monotonic()
            require(remaining > 0, 'relay readiness deadline exceeded')
            return dict(config, limits=dict(config['limits'], timeout_seconds=min(5, remaining)))
        limited = bounded_config()
        items = run_json(DOCKER + ['container', 'inspect', identifier], limited)
        item = next((item for item in items if item.get('Id') == identifier), None)
        require(item is not None and item.get('State', {}).get('Running'), 'staging relay exited before readiness')
        try:
            limited = bounded_config()
            health = run_json(['curl', '--noproxy', '*', '--proxy', '', '--fail', '--silent', '--show-error',
                               '--max-time', str(limited['limits']['timeout_seconds']),
                               'http://127.0.0.1:' + str(config['target']['ports']['health']) + '/_readiness'], limited)
            if isinstance(health, dict) and health.get('status') == 'ready':
                return
        except (Refused, ValueError):
            pass
        remaining = deadline - time.monotonic()
        require(remaining > 0, 'relay readiness deadline exceeded')
        time.sleep(min(0.25, remaining))
    raise Refused('relay readiness attempts exhausted')


def executable_state_gate(config, ids):
    deletion = target_sql(config, ids, """SELECT json_build_object('crew_staging_startup_gate',
(SELECT count(*) FROM communities WHERE deletion_state <> 'active' OR deleted_at IS NOT NULL)
+ (SELECT count(*) FROM community_deletion_requests WHERE completed_at IS NULL)
+ (SELECT count(*) FROM community_serving_write_leases)
+ (SELECT count(*) FROM community_deletion_executor_heartbeats))""")
    require(isinstance(deletion, dict) and deletion.get('crew_staging_startup_gate') == 0,
            'community deletion state requires separate policy before launch')
    tables = ','.join("'" + name + "'" for name in EXCLUDED if name not in ('communities', 'users'))
    sql = """DO $$ DECLARE tab text; total bigint; BEGIN
FOR tab IN SELECT unnest(ARRAY[""" + tables + """]) LOOP
IF to_regclass('public.' || tab) IS NOT NULL THEN
EXECUTE format('SELECT count(*) FROM public.%I', tab) INTO total;
IF total <> 0 THEN RAISE EXCEPTION 'executable state must be empty'; END IF;
END IF; END LOOP;
IF EXISTS (SELECT 1 FROM events WHERE kind=30300 AND not_before IS NOT NULL
AND delivered_at IS NULL AND deleted_at IS NULL) THEN RAISE EXCEPTION 'pending reminder'; END IF;
IF EXISTS (SELECT 1 FROM channels WHERE ttl_deadline IS NOT NULL AND archived_at IS NULL)
THEN RAISE EXCEPTION 'pending channel expiry'; END IF;
END $$;"""
    execute(config, 'exec', ids['postgres'], 'psql', '-X', '-v', 'ON_ERROR_STOP=1',
            '-U', 'staging', '-d', config['target']['database'], '-c', sql)


def verify_assets(config, ids, manifest):
    for name in ('media', 'git'):
        destination = '/restore/media' if name == 'media' else '/restore/git'
        descriptor, filename = tempfile.mkstemp(prefix='asset-readback-', suffix='.tar', dir=path(config['target']['root']))
        file = Path(filename)
        try:
            with os.fdopen(descriptor, 'wb') as stream:
                argv = DOCKER + ['exec', ids['postgres'], 'timeout', '-s', 'KILL',
                                 str(max(1, int(config['limits']['timeout_seconds']) - 1)),
                                 'tar', '-cf', '-', '-C', destination, '.']
                run(argv, config['limits'], output=stream, byte_limit=config['limits']['snapshot_bytes'])
            actual = archive_inventory(file, config['limits']['snapshot_bytes'], config['limits']['timeout_seconds'])
            require(actual == manifest['assets'][name], 'persisted ' + name + ' asset readback mismatch')
        finally:
            file.unlink(missing_ok=True)


def restore(config, baseline):
    # Baseline errors must precede even target process inspection.
    directory, manifest = load_baseline(config, baseline)
    migration_plan(config)
    communities = json.loads((directory / "communities.json").read_text())
    selected = config["source"].get("community_id")
    require(any(item.get("id") == selected for item in communities), "recorded source community missing")
    ids = owned_resources(config)
    inspect_plan(config)
    port_preflight(config, ids)
    root = path(config["target"]["root"])
    require(root.is_dir(), "target root missing")
    with operation_lock(root):
        return restore_locked(config, baseline, directory, manifest, ids, root, communities, selected)


def restore_locked(config, baseline, directory, manifest, ids, root, communities, selected):
    write_json(root / "state.json", {"status": "UNAVAILABLE", "baseline_id": baseline, "container_ids": ids})
    # A stopped relay cannot serve partially restored state. Only recorded IDs.
    execute(config, "stop", "--time", "10", ids["relay"], ids["media"], ids["redis"])
    execute(config, "start", ids["postgres"])
    wait_postgres(config, ids['postgres'])
    database = config["target"]["database"]
    execute(config, "exec", ids["postgres"], "dropdb", "-U", "staging", "--if-exists", "--force", database)
    execute(config, "exec", ids["postgres"], "createdb", "-U", "staging", database)
    execute(config, "exec", ids["postgres"], "psql", "-X", "-v", "ON_ERROR_STOP=1",
            "-U", "staging", "-d", "postgres", "-c",
            'REVOKE CONNECT ON DATABASE "' + database + '" FROM PUBLIC')
    execute(config, "cp", str(directory / "database.dump"), ids["postgres"] + ":/tmp/crew-staging.dump")
    restore_args = ["exec", ids["postgres"], "pg_restore", "-U", "staging", "--dbname", database,
                    "--no-owner", "--no-acl", "--exit-on-error"]
    execute(config, *restore_args, "--section=pre-data", "/tmp/crew-staging.dump")
    authority = urlsplit(config["target"]["relay"]).netloc
    for item in communities:
        if item["id"] == selected:
            item["host"] = authority
    # This metadata contains no private key and does not touch signed events.
    encoded = json.dumps(communities).replace("'", "''")
    sql = """INSERT INTO communities
(id,host,created_at,icon,archived_at,deletion_state,deletion_fence_generation,deleted_at)
SELECT id,host,created_at,icon,archived_at,deletion_state,deletion_fence_generation,deleted_at
FROM jsonb_to_recordset('""" + encoded + """'::jsonb) AS c(id uuid,host text,created_at timestamptz,
icon text,archived_at timestamptz,deletion_state text,deletion_fence_generation bigint,deleted_at timestamptz);
SELECT '{}'::json;"""
    target_sql(config, ids, sql)
    users = json.loads((directory / 'users.json').read_text())
    require(isinstance(users, list) and all(isinstance(user, dict) and 'okta_user_id' not in user for user in users),
            'user snapshot contains excluded external identity linkage')
    encoded_users = json.dumps(users).replace("'", "''")
    target_sql(config, ids, "INSERT INTO users SELECT * FROM jsonb_populate_recordset(NULL::users, '"
               + encoded_users + "'::jsonb); SELECT '{}'::json;")
    execute(config, *restore_args, "--section=data", "/tmp/crew-staging.dump")
    execute(config, *restore_args, "--section=post-data", "/tmp/crew-staging.dump")
    apply_migrations(config, ids["postgres"], root)
    require(target_sql(config, ids, EVENTS_SQL) == manifest["events"], "restored signed events differ from baseline")
    executable_state_gate(config, ids)
    for name in ("media", "git"):
        destination = "/restore/" + name
        execute(config, "exec", ids["postgres"], "find", destination, "-mindepth", "1", "-delete")
        execute(config, "cp", str(directory / (name + ".tar")), ids["postgres"] + ":/tmp/" + name + ".tar")
        execute(config, "exec", ids["postgres"], "tar", "-xf", "/tmp/" + name + ".tar", "-C", destination)
    verify_assets(config, ids, manifest)
    execute(config, "start", ids["redis"], ids["media"])
    execute(config, "exec", ids["redis"], "redis-cli", "FLUSHALL")
    # Reinspect before the only operation that exposes restored data.
    owned_resources(config)
    execute(config, "start", ids["relay"])
    try:
        wait_relay(config, ids['relay'])
    except Exception:
        try:
            execute(config, 'stop', '--time', '10', ids['relay'])
        except Exception:
            write_json(root / 'state.json', {'status': 'UNAVAILABLE', 'baseline_id': baseline,
                                             'container_ids': ids, 'relay_cleanup': 'STOP_FAILED'})
            raise
        raise
    write_json(root / "state.json", {"status": "RESTORED_UNVERIFIED", "baseline_id": baseline,
                                     "container_ids": ids, "events": manifest["events"]})
    return {"status": "RESTORED_UNVERIFIED", "baseline_id": baseline}


def stop(config):
    ids = owned_resources(config)
    with operation_lock(path(config["target"]["root"])):
        execute(config, "stop", "--time", "10", *ids.values())
    return {"status": "STOPPED", "container_ids": ids}
