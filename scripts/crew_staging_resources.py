"""Inspect the fully resolved Compose topology before any staging write."""

import os
from pathlib import Path

from crew_staging_config import on_server, path, require
from crew_staging_process import run_json


LABEL = "com.nuncio.crew.staging"
COMPOSE = Path(__file__).resolve().parent.parent / "docker-compose.crew-staging.yml"

VOLUME_DESTINATIONS = {
    "postgres": {"postgres": "/var/lib/postgresql/data", "media": "/restore/media", "git": "/restore/git"},
    "redis": {"redis": "/data"}, "media": {"media": "/data"}, "relay": {"git": "/srv/git"},
}


def compose_env(config):
    target = config["target"]
    env = dict(os.environ)
    # Do not let an inherited remote daemon change the host being inspected.
    for key in ("DOCKER_HOST", "DOCKER_CONTEXT", "COMPOSE_FILE", "COMPOSE_PROFILES", "COMPOSE_PROJECT_NAME"):
        env.pop(key, None)
    values = {"CREW_STAGING_ID": config["environment_id"],
              "CREW_STAGING_IP": target["bind_ip"],
              "CREW_STAGING_ROOT": target["root"],
              "CREW_STAGING_CPUS": str(config["limits"]["cpus"]),
              "CREW_STAGING_MEMORY": str(config["limits"]["memory_mb"]) + "m"}
    for name, value in target["ports"].items():
        values["CREW_STAGING_" + name.upper() + "_PORT"] = str(value)
    for name, value in config["images"].items():
        values["CREW_STAGING_" + name.upper() + "_IMAGE"] = value
    env.update(values)
    return env


def compose_argv(config, *args):
    return ["docker", "--host", "unix:///var/run/docker.sock", "compose", "--project-name",
            config["environment_id"], "--file", str(COMPOSE), *args]


def inspect_plan(config):
    require(on_server(config), "server inspection must run on the named server")
    env = compose_env(config)
    daemon = run_json(["docker", "--host", "unix:///var/run/docker.sock", "info", "--format", "{{json .}}"], config, env)
    require(daemon.get("Name") == config["server_host"], "Docker daemon host mismatch")
    resolved = run_json(compose_argv(config, "config", "--format", "json"), config, env)
    validate_compose(config, resolved)
    return resolved


def validate_compose(config, resolved):
    environment, target = config["environment_id"], config["target"]
    require(resolved.get("name") == environment, "resolved Compose project mismatch")
    volumes = resolved.get("volumes", {})
    require(set(volumes) == set(target["volumes"]), "resolved volume inventory mismatch")
    for key, volume in volumes.items():
        require(volume.get("name") == target["volumes"][key]
                and not volume.get("external") and not volume.get("driver_opts")
                and volume.get("driver", "local") == "local"
                and volume.get("labels", {}).get(LABEL) == environment,
                "resolved volume is shared or unowned")
    networks = resolved.get("networks", {})
    require(len(networks) == 1, "resolved network inventory mismatch")
    for network in networks.values():
        require(network.get("name") == target["network"] and not network.get("external")
                and network.get("driver", "bridge") == "bridge"
                and network.get("labels", {}).get(LABEL) == environment,
                "resolved network is shared or unowned")
    services = resolved.get("services", {})
    require(set(services) == set(config["images"]), "resolved service inventory mismatch")
    for name, service in services.items():
        require(service.get("image") == config["images"][name], "resolved image digest mismatch")
        require(service.get("labels", {}).get(LABEL) == environment, "service owner mismatch")
        require(not service.get("privileged") and not service.get("network_mode")
                and not service.get("pid") and not service.get("devices")
                and not service.get("cap_add"), "unsafe service privileges refused")
        require(float(service.get("cpus", 0)) == config["limits"]["cpus"]
                and int(service.get("mem_limit", 0)) == config["limits"]["memory_mb"] * 1048576,
                "resolved resource limit mismatch")
        require(set(service.get("networks", {})) == set(networks), "service network mismatch")
        for mount in service.get("volumes", []):
            if mount.get("type") == "volume":
                require(mount.get("source") in volumes, "unowned resolved volume mount")
            else:
                require(mount.get("type") == "bind" and mount.get("read_only")
                        and path(mount.get("source", "")) == path(target["root"]) / "secrets/postgres-password"
                        and name == "postgres" and mount.get("target") == "/run/staging/postgres-password",
                        "unowned or writable bind mount refused")
        mounts = service.get("volumes", [])
        actual_mounts = [(mount.get("source"), mount.get("target"))
                         for mount in mounts if mount.get("type") == "volume"]
        require(len(actual_mounts) == len(VOLUME_DESTINATIONS[name])
                and set(actual_mounts) == set(VOLUME_DESTINATIONS[name].items())
                and not any(mount.get("read_only") for mount in mounts if mount.get("type") == "volume"),
                "required resolved mount inventory changed")
        ports = service.get("ports", [])
        if name == "relay":
            expected = {(target["bind_ip"], target["ports"]["relay"], 3000),
                        ("127.0.0.1", target["ports"]["health"], 8080),
                        ("127.0.0.1", target["ports"]["metrics"], 9102)}
            actual = {(p.get("host_ip"), int(p.get("published", 0)), p.get("target")) for p in ports}
            require(len(ports) == 3 and actual == expected, "resolved relay binding mismatch")
        elif name == "postgres":
            require(len(ports) == 1 and ports[0].get("host_ip") == "127.0.0.1"
                    and int(ports[0].get("published", 0)) == target["ports"]["postgres"]
                    and ports[0].get("target") == 5432, "PostgreSQL fixture binding must be loopback only")
        else:
            require(not ports, "administrative binding must remain internal")
