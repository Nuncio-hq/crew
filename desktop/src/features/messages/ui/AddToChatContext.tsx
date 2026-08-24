import * as React from "react";

type AddToChatContextValue = {
  addSelection: (text: string) => boolean;
  registerComposer: (handler: (text: string) => boolean) => () => void;
};

const AddToChatContext = React.createContext<AddToChatContextValue | null>(
  null,
);

export function AddToChatProvider({ children }: { children: React.ReactNode }) {
  const composerRef = React.useRef<((text: string) => boolean) | null>(null);
  const registerComposer = React.useCallback(
    (handler: (text: string) => boolean) => {
      composerRef.current = handler;
      return () => {
        if (composerRef.current === handler) composerRef.current = null;
      };
    },
    [],
  );
  const addSelection = React.useCallback((text: string) => {
    const composer = composerRef.current;
    if (!composer) return false;
    return composer(text);
  }, []);
  const value = React.useMemo(
    () => ({ addSelection, registerComposer }),
    [addSelection, registerComposer],
  );
  return (
    <AddToChatContext.Provider value={value}>
      {children}
    </AddToChatContext.Provider>
  );
}

export function useAddToChat() {
  return React.useContext(AddToChatContext);
}
