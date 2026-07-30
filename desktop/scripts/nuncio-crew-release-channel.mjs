import { fileURLToPath } from "node:url";

const RELEASE_BASE = "https://github.com/Nuncio-hq/crew/releases/download";
const EXACT_COMMIT = /^[0-9a-f]{40}$/;
const DEV_VERSION = /^v(\d+\.\d+\.\d+-dev(?:\.\d+)?)$/;
const STABLE_VERSION = /^v(\d+\.\d+\.\d+)$/;

function endpoint(rollingTag) {
  return `${RELEASE_BASE}/${rollingTag}/latest.json`;
}

export function classifyNuncioCrewRelease(tag) {
  const devMatch = DEV_VERSION.exec(tag);
  if (devMatch) {
    return {
      version: devMatch[1],
      channel: "dev",
      prerelease: true,
      rollingTags: ["nuncio-crew-dev-latest"],
      updaterEndpoint: endpoint("nuncio-crew-dev-latest"),
    };
  }

  const stableMatch = STABLE_VERSION.exec(tag);
  if (stableMatch) {
    return {
      version: stableMatch[1],
      channel: "stable",
      prerelease: false,
      rollingTags: ["nuncio-crew-stable-latest", "nuncio-crew-dev-latest"],
      updaterEndpoint: endpoint("nuncio-crew-stable-latest"),
    };
  }

  throw new Error(
    `Unsupported release version "${tag}"; use vX.Y.Z-dev[.N] or vX.Y.Z`,
  );
}

export function validateNuncioCrewReleaseRequest({ version, channel, ref }) {
  const release = classifyNuncioCrewRelease(version);
  if (release.channel !== channel) {
    throw new Error(
      `Version ${version} belongs to ${release.channel}, not ${channel}`,
    );
  }
  if (!EXACT_COMMIT.test(ref)) {
    throw new Error("Release ref must be a full 40-character commit SHA");
  }
  return { ...release, ref };
}

function runCli() {
  const [version, channel, ref] = process.argv.slice(2);
  const release = validateNuncioCrewReleaseRequest({ version, channel, ref });
  console.log(`version=${release.version}`);
  console.log(`channel=${release.channel}`);
  console.log(`prerelease=${release.prerelease}`);
  console.log(`rolling_tags=${release.rollingTags.join(",")}`);
  console.log(`updater_endpoint=${release.updaterEndpoint}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    runCli();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
