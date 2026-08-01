import type {
  InstallRuntimeResult,
  InstallStepResult,
} from "@/shared/api/types";

/**
 * Build the user-visible error message for a failed install.
 * When the last step carries an actionable hint, it is shown first,
 * followed by the raw step failure detail.
 *
 * The step detail is truncated for display, so the message ends with a pointer
 * to the install log — which holds every attempt of every step, each record
 * bounded far above the display truncation — when one was written.
 */
export function getInstallErrorMessage(result: InstallRuntimeResult): string {
  const { steps, logPath } = result;
  const lastStep = steps[steps.length - 1];
  if (!lastStep) {
    return withLog("Install failed with no output.", logPath);
  }
  const base = `Step "${lastStep.step}" failed: ${lastStep.stderr || lastStep.stdout || "unknown error"}`;
  const detail = lastStep.hint ? `${lastStep.hint}\n\n${base}` : base;
  return withLog(detail, logPath);
}

function withLog(message: string, logPath: string | null): string {
  return logPath ? `${message}\n\nFull log: ${logPath}` : message;
}

/**
 * Partial-success warning when the requested runtime installed but a purged
 * sibling failed `adapter-repair`. Backend keeps top-level `success: true` so
 * the primary install is not reported as failed; UI must still surface every
 * purged sibling's outcome.
 */
export function getFailedAdapterRepairWarning(
  steps: InstallStepResult[],
): string | null {
  const failed = steps.filter(
    (step) => step.step === "adapter-repair" && !step.success,
  );
  if (failed.length === 0) {
    return null;
  }
  return failed
    .map((step) => {
      const hint = step.hint?.trim();
      if (hint) {
        return hint;
      }
      return `Step "${step.step}" failed: ${step.stderr || step.stdout || "unknown error"}`;
    })
    .join("\n\n");
}

/**
 * Shared mapping for install surfaces (catalog detail + runtime row).
 * Hard failure → error; primary ok with failed sibling repair → warning.
 */
export function getInstallOutcomeMessages(result: InstallRuntimeResult): {
  error: string | null;
  warning: string | null;
} {
  if (!result.success) {
    return {
      error: getInstallErrorMessage(result),
      warning: null,
    };
  }
  return {
    error: null,
    warning: getFailedAdapterRepairWarning(result.steps),
  };
}
