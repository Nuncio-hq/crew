import { CrewAddProjectFlow } from "./crew-add-project-flow";
import { ProjectsView } from "./ProjectsView";

export function CrewProjectsScreen() {
  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <CrewAddProjectFlow>
        {(chooseFolder) => <ProjectsView onCreateRepository={chooseFolder} />}
      </CrewAddProjectFlow>
    </div>
  );
}
