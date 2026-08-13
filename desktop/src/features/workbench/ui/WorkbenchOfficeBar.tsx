import { OFFICE_EXPLANATION } from "../lib/workbenchOfficeFilter";

export function WorkbenchOfficeBar() {
  return (
    <div
      className="border-b border-border/60 bg-muted/30 px-4 py-2 text-sm text-muted-foreground"
      data-testid="workbench-office-bar"
    >
      {OFFICE_EXPLANATION}
    </div>
  );
}
