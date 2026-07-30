import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

function minisignPayload(value, label) {
  const trimmed = value.trim();
  const decodedOuter = Buffer.from(trimmed, "base64").toString("utf8");
  const minisign = trimmed.includes("untrusted comment:")
    ? trimmed
    : decodedOuter;
  const payload = minisign
    .split(/\r?\n/)
    .find((line) => line && !line.includes("comment:"));
  if (!payload) throw new Error(`${label} has no minisign payload`);

  const bytes = Buffer.from(payload, "base64");
  if (bytes.length < 10) throw new Error(`${label} payload is too short`);
  return bytes;
}

export function updaterKeyId(publicKey) {
  const bytes = minisignPayload(publicKey, "Updater public key");
  if (bytes.subarray(0, 2).toString("ascii") !== "Ed") {
    throw new Error("Updater public key has an unsupported algorithm");
  }
  return bytes.subarray(2, 10).toString("hex");
}

export function verifyUpdaterKeyId(publicKey, signature) {
  const expected = updaterKeyId(publicKey);
  const signatureBytes = minisignPayload(signature, "Updater signature");
  const algorithm = signatureBytes.subarray(0, 2).toString("ascii");
  if (!["Ed", "ED"].includes(algorithm)) {
    throw new Error("Updater signature has an unsupported algorithm");
  }
  const actual = signatureBytes.subarray(2, 10).toString("hex");
  if (actual !== expected) {
    throw new Error(
      `Updater signature key ${actual} does not match public key ${expected}`,
    );
  }
  return expected;
}

function main() {
  const signaturePath = process.argv[2];
  const publicKey = process.env.UPDATER_PUBLIC_KEY;
  if (!signaturePath || !publicKey) {
    throw new Error(
      "Expected UPDATER_PUBLIC_KEY and a signature file path argument",
    );
  }
  const signature = readFileSync(signaturePath, "utf8");
  const keyId = verifyUpdaterKeyId(publicKey, signature);
  console.log(`Verified updater signature key ID ${keyId}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
