import { open } from "@tauri-apps/plugin-dialog";

export async function chooseProjectWorkspaceFolder(): Promise<string | null> {
  return open({
    directory: true,
    multiple: false,
    title: "Select Project workspace",
  });
}
