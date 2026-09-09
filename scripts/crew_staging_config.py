"""Fail-closed configuration checks shared by the Crew staging CLI."""

import ipaddress
import os
from pathlib import Path
import re
import socket
from urllib.parse import urlsplit


class Refused(Exception):
    """A staging precondition failed without exposing private input."""


def require(condition, reason):
    if not condition:
        raise Refused(reason)


def path(value):
    require(isinstance(value, str) and Path(value).is_absolute(), "absolute path required")
    return Path(value).resolve()


def overlaps(a, b):
    a, b = path(str(a)), path(str(b))
    return a == b or a in b.parents or b in a.parents


def endpoint(value):
    require(isinstance(value, str), "relay URL required")
    parsed = urlsplit(value)
    require(parsed.scheme in ("http", "https", "ws", "wss") and parsed.hostname,
            "invalid relay URL")
    require(not parsed.username and not parsed.password and not parsed.query and not parsed.fragment,
            "embedded credentials or URL parameters refused")
    require(parsed.path in ("", "/"), "relay URL path refused")
    host = parsed.hostname.lower().rstrip(".")
    try:
        address = ipaddress.ip_address(host)
        host = "loopback" if address.is_loopback else str(address)
    except ValueError:
        if host == "localhost":
            host = "loopback"
    port = parsed.port or (443 if parsed.scheme in ("https", "wss") else 80)
    return host, port


def validate(config):
    require(isinstance(config, dict) and config.get("version") == 1, "config version 1 required")
    try:
        environment = config["environment_id"]
        require(isinstance(environment, str) and re.fullmatch(r"crew-staging-[a-z0-9-]{1,40}", environment),
                "task-owned environment ID required")
        source, target = config["source"], config["target"]
        require(source["host"] == config["server_host"] and bool(source["host"]), "source host required")
        require(endpoint(source["relay"]) != endpoint(target["relay"]), "source and target relay coincide")
        require(re.fullmatch(r"[a-zA-Z0-9_]{1,63}", target["database"])
                and target["database"].startswith(environment.replace("-", "_"))
                and target["database"] != source["database"], "source and target database must differ")
        require(re.fullmatch(r"[a-zA-Z0-9_-]+", source["postgres_container"]), "source container required")
        ports = target["ports"]
        require(set(ports) == {"relay", "health", "metrics", "postgres"}
                and all(type(p) is int and 1024 <= p <= 65535 for p in ports.values())
                and len(set(ports.values())) == 4, "four distinct bounded ports required")
        require(endpoint(target["relay"])[1] == ports["relay"], "relay ports mismatch")
        bind = ipaddress.ip_address(target["bind_ip"])
        require(bind in ipaddress.ip_network("100.64.0.0/10"), "relay binding must be Tailscale IPv4")
        require(endpoint(target["relay"])[0] == str(bind), "relay host must match verified binding")
        volumes = target["volumes"]
        require(set(volumes) == {"postgres", "redis", "media", "git"}, "four storage volumes required")
        require(all(isinstance(v, str) and v.startswith(environment + "-") for v in volumes.values())
                and len(set(volumes.values())) == 4
                and not set(volumes.values()).intersection(source["volumes"]), "shared or unowned volume refused")
        require(target["network"] == environment + "-net", "owned network required")
        require(bool(source["paths"]), "source storage paths required")
        for original in source["paths"]:
            require(not overlaps(original, target["root"]), "source and target path overlap")
            require(not overlaps(original, config["snapshot_root"]), "source and baseline path overlap")
        require(not overlaps(target["root"], config["snapshot_root"]), "baseline and target path overlap")
        baseline = path(config["snapshot_root"])
        require(not any((parent / ".git").exists() for parent in (baseline, *baseline.parents)),
                "baseline must be outside every Git checkout")
        mac = config["mac"]
        roots = [mac[k] for k in ("app_data", "profiles", "workspaces")]
        require(bool(mac["source_roots"]), "installed source paths required")
        for index, root in enumerate(roots):
            for other in mac["source_roots"] + roots[:index]:
                require(not overlaps(root, other), "Mac source and writable path overlap")
        limits = config["limits"]
        for key, maximum in (("timeout_seconds", 300), ("output_bytes", 1048576),
                             ("cpus", 4), ("memory_mb", 4096), ("snapshot_bytes", 10737418240)):
            require(type(limits[key]) in (int, float) and 0 < limits[key] <= maximum,
                    "invalid resource limit: " + key)
        auth = config["auth"]
        require(auth["mode"] in ("real", "fixture-only"), "explicit auth mode required")
        require(re.fullmatch(r"[0-9a-f]{64}", auth["admin_pubkey"]), "staging admin public identity required")
        require(bool(auth["secret_refs"]), "secret references required")
        for value in auth["secret_refs"].values():
            path(value)
        require(re.fullmatch(r"[0-9a-f]{40}", config["build_sha"]), "exact build SHA required")
        require(isinstance(config["schema_version"], str) and bool(config["schema_version"]), "schema version required")
        images = config["images"]
        require(set(images) == {"relay", "postgres", "redis", "media"}, "four pinned images required")
        require(all(re.fullmatch(r"(?:[a-zA-Z0-9./:_-]+@)?sha256:[0-9a-f]{64}", v) for v in images.values()),
                "image digests required")
    except (KeyError, TypeError, ValueError):
        raise Refused("missing or malformed manifest field") from None
    return config


def on_server(config):
    return socket.gethostname().lower().rstrip(".") == config["server_host"].lower().rstrip(".")


def public_plan(config):
    return {"environment_id": config["environment_id"], "status": "UNRESOLVED",
            "reason": "server resource inspection required", "server_host": config["server_host"],
            "target": {key: config["target"][key] for key in ("relay", "bind_ip", "ports", "database", "network", "volumes", "root")},
            "mac": {key: config["mac"][key] for key in ("source_roots", "app_data", "profiles", "workspaces")},
            "snapshot_root": config["snapshot_root"], "limits": config["limits"],
            "auth_class": config["auth"]["mode"], "admin_pubkey": config["auth"]["admin_pubkey"],
            "build_sha": config["build_sha"], "schema_version": config["schema_version"],
            "images": config["images"],
            "aggregate_limits": {"cpus": config["limits"]["cpus"] * 4,
                                 "memory_mb": config["limits"]["memory_mb"] * 4}}
