import path from "node:path";
import { fileURLToPath } from "node:url";
import { runPaneResponsiveCheck } from "../../scripts/check-pane-responsive-core.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..");

await runPaneResponsiveCheck({
  projectRoot,
  label: "Desktop",
  scriptPath: "desktop/scripts/check-pane-responsive.mjs",
  p1Overrides: new Set(),
  p3Overrides: new Set(),
});
