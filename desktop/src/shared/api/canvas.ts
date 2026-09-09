import { invokeTauri } from "./tauri";
import type { CanvasResponse } from "./types";

type RawCanvasResponse = {
  content: string | null;
  event_id?: string | null;
  definitions?: { role_label: string; definition: string }[];
  contact_pubkey?: string | null;
  crew_authority?: "absent" | "owner" | "foreign";
  crew_parse_state?: "absent" | "valid" | "invalid";
  stored_assignments?: Record<string, string>;
  stored_routing?: Record<string, string>;
  stored_capabilities?: Record<string, string[]>;
  updated_at: number | null;
  author: string | null;
  routing: {
    work_type: string;
    role_label: string;
    holders: string[];
    unheld_message: string | null;
  }[];
  assignments?: {
    agent_pubkey: string;
    role_label: string;
  }[];
  dev_mcp_granted: boolean | null;
  crew_parse_error: string | null;
};

export async function getCanvas(channelId: string): Promise<CanvasResponse> {
  const response = await invokeTauri<RawCanvasResponse>("get_canvas", {
    channelId,
  });
  return {
    content: response.content,
    eventId: response.event_id ?? null,
    definitions: (response.definitions ?? []).map((entry) => ({
      roleLabel: entry.role_label,
      definition: entry.definition,
    })),
    contactPubkey: response.contact_pubkey ?? null,
    crewAuthority: response.crew_authority ?? "absent",
    crewParseState: response.crew_parse_state ?? "absent",
    storedAssignments: response.stored_assignments ?? {},
    storedRouting: response.stored_routing ?? {},
    storedCapabilities: response.stored_capabilities ?? {},
    updatedAt: response.updated_at ?? null,
    author: response.author ?? null,
    routing: response.routing.map((entry) => ({
      workType: entry.work_type,
      roleLabel: entry.role_label,
      holders: entry.holders,
      unheldMessage: entry.unheld_message,
    })),
    assignments: (response.assignments ?? []).map((entry) => ({
      agentPubkey: entry.agent_pubkey,
      roleLabel: entry.role_label,
    })),
    devMcpGranted: response.dev_mcp_granted,
    crewParseError: response.crew_parse_error,
  };
}
