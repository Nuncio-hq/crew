import { createContext, useContext, useState } from "react";

export const folderSamples = [
  {
    id: "crew",
    name: "crew",
    path: "/sample/workspaces/crew",
    machine: "This Mac",
    kind: "git",
    branch: "main",
    changes: 3,
    available: true,
  },
  {
    id: "heardback",
    name: "heardback",
    path: "/sample/workspaces/heardback",
    machine: "This Mac",
    kind: "git",
    branch: "main",
    changes: 0,
    available: true,
  },
  {
    id: "research",
    name: "Research",
    path: "/sample/documents/Research",
    machine: "This Mac",
    kind: "folder",
    available: true,
  },
  {
    id: "remote",
    name: "crew-server",
    path: "/sample/server/crew",
    machine: "dev-server",
    kind: "git",
    branch: "main",
    available: false,
  },
];
export const commonChannels = [
  { name: "general", description: "Company-wide conversations and updates" },
  { name: "announcements", description: "News everyone should know" },
];
const initialProjects = [
  {
    name: "NuncioCrew",
    description: "A shared place to build, discuss and ship NuncioCrew.",
    channels: [
      {
        name: "product",
        description: "Product direction, decisions and feedback",
        home: true,
      },
      {
        name: "engineering",
        description: "Implementation, reviews and releases",
      },
    ],
    workspaces: [folderSamples[0]],
  },
  {
    name: "HeardBack",
    description: "Build a better path from application to reply.",
    channels: [
      {
        name: "marketing",
        description: "Campaigns, content and audience decisions",
        home: true,
      },
      { name: "customers", description: "Feedback and customer follow-ups" },
      { name: "general", description: "Shared conversations" },
    ],
    workspaces: [folderSamples[1]],
  },
  {
    name: "Didit",
    description: "A place for the next idea to take shape.",
    channels: [
      { name: "general", description: "Start a conversation", home: true },
    ],
    workspaces: [],
  },
];
const ProjectContext = createContext(null);
export function ProjectProvider({ children }) {
  const [projects, setProjects] = useState(initialProjects);
  function update(name, fn) {
    setProjects((ps) => ps.map((p) => (p.name === name ? fn(p) : p)));
  }
  const value = {
    projects,
    reset: () => setProjects(initialProjects),
    add: (p) => setProjects((ps) => [...ps, p]),
    addChannel: (project, channel) =>
      update(project, (p) => ({ ...p, channels: [...p.channels, channel] })),
    setWorkspace: (project, folder, previousId) =>
      update(project, (p) => ({
        ...p,
        workspaces: [
          ...p.workspaces.filter(
            (w) => w.id !== previousId && w.id !== folder?.id,
          ),
          ...(folder ? [folder] : []),
        ],
      })),
  };
  return (
    <ProjectContext.Provider value={value}>{children}</ProjectContext.Provider>
  );
}
export const useProjects = () => useContext(ProjectContext);
