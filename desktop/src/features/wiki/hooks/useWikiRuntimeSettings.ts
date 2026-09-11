import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

import {
  sameOwnerOperationScope,
  type OwnerOperationScope,
  type ScopedOwnerOperation,
} from "@/shared/api/ownerOperations";

/** A Wiki-only runtime binding; Hermes owns its model in the named profile. */
export type WikiRuntimeSelection = {
  runtimeId: string;
  model?: string | null;
  profile?: string | null;
};

type ScopedWikiRuntimeSelection =
  ScopedOwnerOperation<WikiRuntimeSelection | null>;

export const wikiRuntimeSettingsQueryKey = (
  coordinate: string,
  expected: OwnerOperationScope,
) =>
  [
    "wiki-runtime-settings",
    coordinate,
    expected.scope.owner,
    expected.scope.community,
    expected.workspace_generation,
    expected.identity_generation,
  ] as const;

function ensureScope(
  actual: OwnerOperationScope,
  expected: OwnerOperationScope,
): void {
  if (!sameOwnerOperationScope(actual, expected)) {
    throw new Error(
      "The active owner or community changed while reading Wiki runtime settings.",
    );
  }
}

export function useWikiRuntimeSettings(
  coordinate: string | undefined,
  expected: OwnerOperationScope | undefined,
  options: { enabled?: boolean } = {},
) {
  const enabled = Boolean(coordinate && expected) && (options.enabled ?? true);
  return useQuery({
    enabled,
    queryKey:
      coordinate && expected
        ? wikiRuntimeSettingsQueryKey(coordinate, expected)
        : (["wiki-runtime-settings", "disabled"] as const),
    queryFn: async () => {
      if (!coordinate || !expected) {
        throw new Error("Wiki runtime settings require an owner scope.");
      }
      const result = await invoke<ScopedWikiRuntimeSelection>(
        "wiki_runtime_settings_get",
        { expected, coordinate },
      );
      ensureScope(result.token, expected);
      return result.value;
    },
    staleTime: 30_000,
  });
}

export function useSetWikiRuntimeSettings() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (input: {
      coordinate: string;
      expected: OwnerOperationScope;
      selection: WikiRuntimeSelection;
    }) => {
      const result = await invoke<ScopedOwnerOperation<WikiRuntimeSelection>>(
        "wiki_runtime_settings_set",
        input,
      );
      ensureScope(result.token, input.expected);
      return result.value;
    },
    onSuccess: (selection, input) => {
      queryClient.setQueryData(
        wikiRuntimeSettingsQueryKey(input.coordinate, input.expected),
        selection,
      );
    },
  });
}
