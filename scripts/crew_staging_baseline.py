"""Private immutable database/asset baselines; never changes source storage."""

from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import tarfile
import time
import tempfile

from crew_staging_config import Refused, on_server, path, require
from crew_staging_process import run, run_json
from crew_staging_archive import capture_local
from crew_staging_source import inspect_source, source_command
from crew_staging_migrations import migration_plan
from crew_staging_lock import operation_lock


# These are executable state or credential metadata, not signed Nostr events.
# Communities are reconstructed from a separate key-free column allowlist.
EXCLUDED = (
    "communities", "users", "api_tokens", "relay_invites", "replica_heartbeat",
    "workflows", "workflow_runs", "workflow_approvals", "scheduled_workflow_fires",
    "push_gateway_challenges", "push_gateway_installations", "push_gateway_delegations",
    "push_gateway_endpoint_quotas", "push_gateway_delivery_auth_replays",
    "push_gateway_delivery_request_replays", "push_leases", "push_match_queue",
    "push_wake_outbox", "subscriptions",
    "relay_admin_actions", "relay_admin_outbox",
)


def fingerprint(config):
    return hashlib.sha256(json.dumps(config, sort_keys=True).encode()).hexdigest()


def binding_fingerprint(config):
    # The source baseline outlives rebuilt relay containers and newer reviewed
    # branch migrations. Its data destination and isolation roots remain fixed.
    return fingerprint({"environment_id": config["environment_id"], "source": config["source"],
                        "target": {key: config["target"][key] for key in
                                   ("relay", "bind_ip", "ports", "database", "network", "volumes", "root")},
                        "snapshot_root": config["snapshot_root"],
                        "mac": {key: config["mac"][key] for key in ("app_data", "profiles", "workspaces")},
                        "admin_pubkey": config["auth"]["admin_pubkey"], "auth_mode": config["auth"]["mode"]})


def digest(file, byte_limit=10737418240, deadline=None):
    deadline = deadline or time.monotonic() + 300
    before = file.stat()
    require(before.st_size <= byte_limit, 'baseline file exceeds byte budget')
    checksum = hashlib.sha256()
    total = 0
    with file.open("rb") as stream:
        for block in iter(lambda: stream.read(65536), b""):
            total += len(block)
            require(total <= byte_limit and time.monotonic() < deadline, 'baseline hashing budget exceeded')
            checksum.update(block)
    after = file.stat()
    require((before.st_ino, before.st_size, before.st_mtime_ns) == (after.st_ino, after.st_size, after.st_mtime_ns)
            and total == before.st_size, 'baseline changed while hashing')
    return checksum.hexdigest()


def write_json(file, value):
    descriptor, name = tempfile.mkstemp(prefix=file.name + '.', suffix='.tmp', dir=file.parent)
    temporary = Path(name)
    try:
        with os.fdopen(descriptor, 'w', encoding='utf-8') as stream:
            json.dump(value, stream, indent=2, sort_keys=True)
            stream.flush()
            os.fsync(stream.fileno())
        temporary.replace(file)
    finally:
        temporary.unlink(missing_ok=True)
    descriptor = os.open(file.parent, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def baseline_path(config, baseline):
    require(isinstance(baseline, str) and re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}", baseline),
            "bounded baseline ID required")
    root = path(config["snapshot_root"])
    candidate = root / baseline
    require(not candidate.is_symlink(), "baseline symlink refused")
    return candidate


def source_sql(config, sql):
    source = config["source"]
    return run_json(source_command(config, "psql", "-X", "-v", "ON_ERROR_STOP=1",
                                   "-U", source["postgres_user"], "-d", source["database"], "-Atc", sql), config)


COMMUNITIES_SQL = """SELECT COALESCE(json_agg(row_to_json(c) ORDER BY c.id), '[]'::json)
FROM (SELECT id,host,created_at,icon,archived_at,deletion_state,deletion_fence_generation,deleted_at
FROM communities) c"""

USERS_SQL = """SELECT COALESCE(json_agg(row_to_json(u) ORDER BY u.community_id,u.pubkey), '[]'::json)
FROM (SELECT community_id,pubkey,nip05_handle,display_name,avatar_url,about,agent_type,capabilities,
created_at,updated_at,deactivated_at,metadata_event_id,agent_owner_pubkey,channel_add_policy FROM users) u"""

EVENTS_SQL = """SELECT json_build_object('event_count', count(*), 'signed_digest',
encode(sha256(convert_to(COALESCE(string_agg(encode(sha256(convert_to(jsonb_build_array(community_id,
encode(id,'hex'),encode(pubkey,'hex'),extract(epoch from created_at),kind,tags,content,
encode(sig,'hex'))::text, 'UTF8')), 'hex'), '' ORDER BY community_id,id), ''), 'UTF8')), 'hex')) FROM events"""


def event_subset_sql(ids):
    require(isinstance(ids, list) and len(ids) <= 500
            and all(isinstance(value, str) and re.fullmatch(r'[a-f0-9]{64}', value) for value in ids),
            'bounded event ID batch required')
    predicate = ','.join("'" + value + "'" for value in ids) or "''"
    return EVENTS_SQL + " WHERE encode(id,'hex') IN (" + predicate + ')'


def archive_inventory(file, limit, timeout=300):
    """Reject links/devices/traversal and checksum each regular asset."""
    result, total, count = {}, 0, 0
    deadline = time.monotonic() + timeout
    with tarfile.open(file, "r:*") as archive:
        for member in archive:
            count += 1
            require(count <= 10000 and time.monotonic() < deadline, "archive count or deadline exceeded")
            item = Path(member.name)
            canonical = item.as_posix()
            require(not item.is_absolute() and ".." not in item.parts and canonical not in result,
                    "unsafe or duplicate asset archive path")
            require(member.isdir() or member.isfile(), "asset symlink, hardlink or device refused")
            if member.isfile():
                total += member.size
                require(total <= limit, "snapshot asset size limit exceeded")
                stream = archive.extractfile(member)
                checksum = hashlib.sha256()
                for block in iter(lambda: stream.read(65536), b""):
                    require(time.monotonic() < deadline, "archive checksum deadline exceeded")
                    checksum.update(block)
                result[canonical] = {"bytes": member.size, "sha256": checksum.hexdigest()}
    return result


def capture_assets(config, directory, suffix):
    assets = config["source"]["assets"]
    require(set(assets) == {"media", "git"}, "explicit media and Git asset sources required")
    inventories = {}
    for kind, source in assets.items():
        remaining = config["limits"]["snapshot_bytes"] - sum(f.stat().st_size for f in directory.iterdir() if f.is_file())
        require(remaining > 0, "total snapshot size limit exceeded")
        output = directory / (kind + suffix + ".tar")
        with output.open("xb") as stream:
            os.chmod(output, 0o600)
            if "container" in source:
                require(re.fullmatch(r"[a-zA-Z0-9_-]+", source["container"]), "invalid asset source container")
                source_path = path(source["path"])
                require(str(source_path) not in ("/", "/data"), "capture only declared asset subtree")
                run(["docker", "--host", "unix:///var/run/docker.sock", "cp",
                     source["container"] + ":" + str(source_path), "-"], config["limits"],
                    output=stream, byte_limit=remaining)
            else:
                root = path(source["path"])
                require(root.is_dir() and root.name == "repos" and root in [path(p) for p in config["source"]["paths"]], "missing declared Git repos asset path")
                capture_local(root, stream, dict(config["limits"], snapshot_bytes=remaining))
            stream.flush()
            os.fsync(stream.fileno())
        inventories[kind] = archive_inventory(output, config["limits"]["snapshot_bytes"], config["limits"]["timeout_seconds"])
    return inventories


def snapshot(config, baseline):
    require(on_server(config), "snapshot must run on the named server")
    root = path(config['snapshot_root'])
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    require(root.stat().st_uid == os.getuid() and root.stat().st_mode & 0o077 == 0,
            'snapshot root must be owner-only')
    with operation_lock(root):
        return snapshot_locked(config, baseline)


def snapshot_locked(config, baseline):
    directory = baseline_path(config, baseline)
    require(not directory.exists(), "baseline ID already exists; no implicit refresh")
    source = config["source"]
    migration_plan(config)
    require(re.fullmatch(r"[a-zA-Z0-9_]+", source.get("postgres_user", "")), "source read user required")
    counts = inspect_source(config)
    directory.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    require(directory.parent.stat().st_uid == os.getuid()
            and directory.parent.stat().st_mode & 0o077 == 0, "snapshot root must be owner-only")
    directory.mkdir(mode=0o700)
    state = {"status": "INCOMPLETE", "baseline_id": baseline,
             "config_fingerprint": fingerprint(config), "binding_fingerprint": binding_fingerprint(config),
             "schema_version": source["schema_version"], "target_schema_at_capture": config["schema_version"],
             "source_schema_version": source["schema_version"],
             "build_sha": config["build_sha"], "created_at": datetime.now(timezone.utc).isoformat(),
             "excluded_table_data": list(EXCLUDED), "source_counts": counts}
    write_json(directory / "manifest.json", state)
    before = capture_assets(config, directory, "")
    communities = source_sql(config, COMMUNITIES_SQL)
    users = source_sql(config, USERS_SQL)
    events = source_sql(config, EVENTS_SQL)
    event_ids = source_sql(config, "SELECT json_build_object('event_ids', COALESCE(json_agg(encode(id,'hex') ORDER BY community_id,id),'[]'::json)) FROM events")["event_ids"]
    require(isinstance(event_ids, list) and all(isinstance(value, str) and re.fullmatch(r"[a-f0-9]{64}", value) for value in event_ids),
            "invalid signed event ID inventory")
    unique_ids = sorted(set(event_ids))
    batches = [unique_ids[start:start + 500] for start in range(0, len(unique_ids), 500)] or [[]]
    event_batches = [{'ids': batch, 'events': source_sql(config, event_subset_sql(batch))} for batch in batches]
    require(bool(communities), "source community metadata unavailable")
    write_json(directory / "communities.json", communities)
    write_json(directory / 'users.json', users)
    argv = source_command(config, "pg_dump", "-U", source["postgres_user"], "-d", source["database"],
                          "--format=custom", "--no-owner", "--no-acl", "--lock-wait-timeout=5000")
    argv += ["--exclude-table-data=public." + name for name in EXCLUDED]
    remaining = config["limits"]["snapshot_bytes"] - sum(f.stat().st_size for f in directory.iterdir() if f.is_file())
    require(remaining > 0, "total snapshot size limit exceeded")
    with (directory / "database.dump").open("xb") as stream:
        os.chmod(directory / "database.dump", 0o600)
        run(argv, config["limits"], output=stream, byte_limit=remaining)
        stream.flush()
        os.fsync(stream.fileno())
    after = capture_assets(config, directory, "-after")
    require(before == after and communities == source_sql(config, COMMUNITIES_SQL)
            and users == source_sql(config, USERS_SQL)
            and events == source_sql(config, EVENTS_SQL),
            "cross-store capture changed; baseline incomplete, retry with a new ID")
    for kind in after:
        (directory / (kind + "-after.tar")).unlink()
    files = ["database.dump", "communities.json", "users.json", "media.tar", "git.tar"]
    require(sum((directory / name).stat().st_size for name in files) <= config["limits"]["snapshot_bytes"],
            "total snapshot size limit exceeded")
    state.update(status="COMPLETE", assets=before, events=events, event_ids=event_ids, event_batches=event_batches,
                 sanitization=['users.okta_user_id omitted; public profiles and signed events preserved'],
                 checksums={name: digest(directory / name, config["limits"]["snapshot_bytes"],
                                         time.monotonic() + config["limits"]["timeout_seconds"]) for name in files},
                 consistency="pre/post assets, communities, sanitized users and signed-event sentinels unchanged; database uses pg_dump snapshot; not a global atomic cross-store snapshot")
    write_json(directory / "manifest.json", state)
    return {"status": "COMPLETE", "baseline_id": baseline, "build_sha": config["build_sha"]}


def load_baseline(config, baseline):
    directory = baseline_path(config, baseline)
    deadline = time.monotonic() + config['limits']['timeout_seconds']
    metadata = directory / 'manifest.json'
    require(not metadata.is_symlink() and metadata.stat().st_size <= 16777216, 'baseline metadata budget exceeded')
    with metadata.open('rb') as stream:
        payload = stream.read(16777217)
    require(len(payload) <= 16777216 and time.monotonic() < deadline, 'baseline metadata budget exceeded')
    state = json.loads(payload)
    require(state.get("status") == "COMPLETE", "incomplete baseline cannot be restored")
    require(state.get("schema_version") == config["source"]["schema_version"], "incompatible baseline source schema")
    require(state.get("binding_fingerprint") == binding_fingerprint(config), "baseline target/config mismatch")
    require(set(state.get("checksums", {})) == {"database.dump", "communities.json", "users.json", "media.tar", "git.tar"},
            "baseline asset manifest incomplete")
    remaining = config['limits']['snapshot_bytes']
    for name, checksum in state["checksums"].items():
        file = directory / name
        require(file.is_file() and not file.is_symlink() and digest(file, remaining, deadline) == checksum,
                "baseline checksum mismatch")
        remaining -= file.stat().st_size
        require(remaining >= 0 and time.monotonic() < deadline, 'baseline aggregate budget exceeded')
    return directory, state
