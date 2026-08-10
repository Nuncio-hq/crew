import type { HermesProfileReadiness } from "@/shared/api/types";

export type HermesProfileReadinessPresentation = {
  label: string;
  tone: "blocking" | "neutral";
  explanation: string;
  repair: string;
  testId: string;
};

export function presentHermesProfileReadiness(
  readiness: HermesProfileReadiness,
): HermesProfileReadinessPresentation {
  switch (readiness.state) {
    case "ready":
      return {
        label: "Ready",
        tone: "neutral",
        explanation: "This profile is ready to start.",
        repair: "No repair needed.",
        testId: "hermes-readiness-ready",
      };
    case "missing":
      return {
        label: "Profile missing",
        tone: "blocking",
        explanation: `Hermes profile '${readiness.profile}' is missing.`,
        repair: "Recreate the profile or change the binding.",
        testId: "hermes-readiness-missing",
      };
    case "broken_config":
      return {
        label: "Config invalid",
        tone: "blocking",
        explanation: `Hermes profile '${readiness.profile}' has invalid configuration.`,
        repair: `Fix config.yaml: ${readiness.diagnostic}`,
        testId: "hermes-readiness-broken-config",
      };
    case "binary_missing":
      return {
        label: "Binary missing",
        tone: "blocking",
        explanation: `Hermes command '${readiness.command}' cannot be run.`,
        repair: "Install Hermes or fix its PATH entry.",
        testId: "hermes-readiness-binary-missing",
      };
    case "auth_unknown":
      return {
        label: "Auth not verified",
        tone: "neutral",
        explanation: "Hermes authentication cannot be verified locally.",
        repair: "Confirm authentication in Hermes before starting.",
        testId: "hermes-readiness-auth-unknown",
      };
  }
}
