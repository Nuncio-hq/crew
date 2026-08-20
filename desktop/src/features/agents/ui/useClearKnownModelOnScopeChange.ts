import * as React from "react";
import { shouldClearKnownModelForSelectionScope } from "./agentConfigOptions";

/** Clears a known model when provider/runtime scope makes it invalid. */
export function useClearKnownModelOnScopeChange({
  open,
  modelFieldVisible = true,
  isCustomModelEditing,
  model,
  provider,
  runtime,
  setModel,
  setIsCustomModelEditing,
}: {
  open: boolean;
  modelFieldVisible?: boolean;
  isCustomModelEditing: boolean;
  model: string;
  provider: string;
  runtime: string;
  setModel: (next: string) => void;
  setIsCustomModelEditing: (next: boolean) => void;
}): void {
  React.useEffect(() => {
    if (
      !open ||
      !modelFieldVisible ||
      isCustomModelEditing ||
      !shouldClearKnownModelForSelectionScope({
        model,
        provider,
        runtime,
      })
    ) {
      return;
    }
    setModel("");
    setIsCustomModelEditing(false);
  }, [
    isCustomModelEditing,
    model,
    modelFieldVisible,
    open,
    provider,
    runtime,
    setIsCustomModelEditing,
    setModel,
  ]);
}
