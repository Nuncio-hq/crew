import * as React from "react";

import type { CrewViewContext } from "@/features/projects/lib/project-view-agent-context";

const ComposerViewContextContext = React.createContext<CrewViewContext | null>(
  null,
);

/**
 * Crew-owned mount for visible-page agent context. Upstream Buzz supplies this
 * payload from its Projects page chrome; Crew supplies it from the channel and
 * thread-focus chrome so channel-first entry stays the only entry point.
 */
export function ComposerViewContextProvider({
  value,
  children,
}: {
  value: CrewViewContext | null;
  children: React.ReactNode;
}) {
  return (
    <ComposerViewContextContext.Provider value={value}>
      {children}
    </ComposerViewContextContext.Provider>
  );
}

export function useComposerViewContext(): CrewViewContext | null {
  return React.useContext(ComposerViewContextContext);
}
