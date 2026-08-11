import { invokeTauri } from "./tauri";
import type { CanvasResponse } from "./types";

type RawCanvasResponse = {
  content: string | null;
  updated_at: number | null;
  author: string | null;
  routing: { work_type: string; role_label: string; holders: string[] }[];
  dev_mcp_granted: boolean | null;
};

export async function getCanvas(channelId: string): Promise<CanvasResponse> {
  const response = await invokeTauri<RawCanvasResponse>("get_canvas", {
    channelId,
  });
  return {
    content: response.content,
    updatedAt: response.updated_at ?? null,
    author: response.author ?? null,
    routing: response.routing.map((entry) => ({
      workType: entry.work_type,
      roleLabel: entry.role_label,
      holders: entry.holders,
    })),
    devMcpGranted: response.dev_mcp_granted,
  };
}
