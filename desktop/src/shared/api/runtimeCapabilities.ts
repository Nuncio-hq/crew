export type RuntimeCapabilities = {
  modelSource: "profileWriteThrough" | "adapterSetting";
  personaDoc: "soulMd" | "none";
  layer3: "append";
};

/** Persona filenames owned by capability descriptors, not render branches. */
export const runtimePersonaDocument = {
  hermes: "SOUL.md",
} as const;

const runtimePersonaKinds: Record<string, RuntimeCapabilities["personaDoc"]> = {
  hermes: "soulMd",
};

export function deriveRuntimeCapabilities(
  runtime:
    | Pick<
        {
          id: string;
          profileArg?: string | null;
          providerLocked?: boolean;
          modelEnvVar: string | null;
        },
        "id" | "profileArg" | "providerLocked" | "modelEnvVar"
      >
    | undefined,
): RuntimeCapabilities {
  const profileWriteThrough = Boolean(
    runtime?.profileArg?.trim() &&
      runtime.providerLocked === true &&
      !runtime.modelEnvVar,
  );
  return {
    modelSource: profileWriteThrough ? "profileWriteThrough" : "adapterSetting",
    personaDoc: runtimePersonaKinds[runtime?.id ?? ""] ?? "none",
    layer3: "append",
  };
}
