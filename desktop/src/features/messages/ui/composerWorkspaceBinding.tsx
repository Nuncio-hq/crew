import * as React from "react";

import {
  DEFAULT_WORKSPACE_BINDING,
  type WorkspaceBindingChoice,
} from "@/features/messages/lib/workspaceBindingSpec";

const ComposerWorkspaceBindingContext =
  React.createContext<WorkspaceBindingChoice>(DEFAULT_WORKSPACE_BINDING);

export function ComposerWorkspaceBindingProvider({
  value,
  children,
}: {
  value: WorkspaceBindingChoice;
  children: React.ReactNode;
}) {
  return (
    <ComposerWorkspaceBindingContext.Provider value={value}>
      {children}
    </ComposerWorkspaceBindingContext.Provider>
  );
}

export function useComposerWorkspaceBinding(): WorkspaceBindingChoice {
  return React.useContext(ComposerWorkspaceBindingContext);
}
