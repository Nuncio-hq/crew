import { createContext, useContext, useState } from "react";
export const sampleChannels = [
  { id: "NuncioCrew/product", label: "NuncioCrew / #product" },
  { id: "NuncioCrew/engineering", label: "NuncioCrew / #engineering" },
  { id: "HeardBack/marketing", label: "HeardBack / #marketing" },
  { id: "HeardBack/customers", label: "HeardBack / #customers" },
  { id: "HeardBack/general", label: "HeardBack / #general" },
];
export const sampleProfiles = ["engineering", "marketing", "research"];
const seeds = [
  {
    id: "Hermes",
    name: "Hermes",
    runtime: "Hermes",
    profile: "engineering",
    model: "",
    status: "Working",
    channels: sampleChannels.map((c) => c.id),
  },
  {
    id: "Codex",
    name: "Codex",
    runtime: "Codex",
    profile: "",
    model: "",
    status: "Available",
    channels: sampleChannels
      .filter((c) => !c.id.endsWith("/customers"))
      .map((c) => c.id),
  },
  {
    id: "Claude",
    name: "Claude",
    runtime: "Claude",
    profile: "",
    model: "",
    status: "Available",
    channels: sampleChannels.map((c) => c.id),
  },
];
const AgentContext = createContext(null);
export function AgentDirectoryProvider({ children }) {
  const [agents, setAgents] = useState(seeds);
  const save = (record) =>
    setAgents((current) =>
      current.some((a) => a.id === record.id)
        ? current.map((a) => (a.id === record.id ? record : a))
        : [...current, record],
    );
  return (
    <AgentContext.Provider
      value={{
        agents: agents.filter((a) => !a.deleted),
        allAgents: agents,
        save,
        remove: (id) =>
          setAgents((current) =>
            current.map((a) =>
              a.id === id
                ? { ...a, deleted: true, channels: [], status: "Removed" }
                : a,
            ),
          ),
        reset: () => setAgents(seeds),
      }}
    >
      {children}
    </AgentContext.Provider>
  );
}
export function useAgentDirectory() {
  const store = useContext(AgentContext);
  return {
    ...store,
    find: (id) => (store?.allAgents || seeds).find((a) => a.id === id),
    displayName: (id) =>
      (store?.allAgents || seeds).find((a) => a.id === id)?.name || id,
  };
}
