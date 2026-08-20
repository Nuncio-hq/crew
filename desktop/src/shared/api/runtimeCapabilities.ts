export type RuntimeCapabilities = {
  modelSource: "profileWriteThrough" | "adapterSetting";
  personaDoc: "soulMd" | "none";
  layer3: "append";
  /**
   * When true, the model picker may offer Cursor-style Auto (persisted as the
   * literal model id `"auto"`) even outside Buzz shared compute. Derived from
   * the catalog runtime id — Cursor's CLI expects `--model auto` at startup.
   */
  selectableAutoModel: boolean;
};

/** Persona filenames owned by capability descriptors, not render branches. */
export const runtimePersonaDocument = {
  hermes: "SOUL.md",
} as const;

/** Wire id Cursor ACP accepts for its automatic model router. */
export const CURSOR_AUTO_MODEL_ID = "auto";

const runtimePersonaKinds: Record<string, RuntimeCapabilities["personaDoc"]> = {
  hermes: "soulMd",
};

const selectableAutoModelRuntimeIds = new Set(["cursor"]);

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
    selectableAutoModel: selectableAutoModelRuntimeIds.has(
      (runtime?.id ?? "").trim(),
    ),
  };
}
