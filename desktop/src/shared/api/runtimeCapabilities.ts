export type RuntimeCapabilities = {
  modelSource: "profileWriteThrough" | "adapterSetting";
  personaDoc: "soulMd" | "none";
  layer3: "append";
};

/** Persona filenames owned by capability descriptors, not render branches. */
export const runtimePersonaDocument = {
  soulMd: "SOUL.md",
  none: null,
} as const;

export function deriveRuntimeCapabilities(
  runtime:
    | Pick<
        {
          profileArg?: string | null;
          providerLocked?: boolean;
          modelEnvVar: string | null;
        },
        "profileArg" | "providerLocked" | "modelEnvVar"
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
    personaDoc: profileWriteThrough ? "soulMd" : "none",
    layer3: "append",
  };
}
