function stringOrNull(value) {
  return typeof value === "string" ? value : null;
}

function normalizeCi(value) {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((entry) => {
      if (typeof entry === "string") {
        return { label: entry, status: "" };
      }
      if (
        entry &&
        typeof entry === "object" &&
        typeof entry.label === "string" &&
        typeof entry.status === "string"
      ) {
        return { label: entry.label, status: entry.status };
      }
      return null;
    })
    .filter(Boolean);
}

/** Map a terminal agent receipt into the fields the timeline card renders. */
export function parseAgentReceipt(content) {
  let payload;
  try {
    payload = JSON.parse(content);
  } catch {
    return null;
  }

  if (
    !payload ||
    typeof payload !== "object" ||
    typeof payload.summary !== "string" ||
    typeof payload.verify !== "string" ||
    !Array.isArray(payload.lights) ||
    !payload.lights.every(
      (light) =>
        light &&
        typeof light === "object" &&
        typeof light.label === "string" &&
        typeof light.status === "string",
    ) ||
    !payload.engineering ||
    typeof payload.engineering !== "object" ||
    Array.isArray(payload.engineering)
  ) {
    return null;
  }

  return {
    summary: payload.summary,
    verify: payload.verify,
    lights: payload.lights.map(({ label, status }) => ({ label, status })),
    engineering: {
      prRef: stringOrNull(payload.engineering.pr_ref),
      branch: stringOrNull(payload.engineering.branch),
      filesChanged: Array.isArray(payload.engineering.files_changed)
        ? payload.engineering.files_changed.filter(
            (file) => typeof file === "string",
          )
        : [],
      ci: normalizeCi(payload.engineering.ci),
    },
    run:
      payload.run &&
      typeof payload.run === "object" &&
      !Array.isArray(payload.run) &&
      typeof payload.run.session_id === "string" &&
      payload.run.session_id.length > 0 &&
      typeof payload.run.turn_id === "string" &&
      payload.run.turn_id.length > 0
        ? {
            sessionId: payload.run.session_id,
            turnId: payload.run.turn_id,
          }
        : null,
  };
}
