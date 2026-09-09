import { invokeTauri } from "@/shared/api/tauri";
import type { RelayEvent } from "@/shared/api/types";

export type WikiSourceGrant = {
  capabilityId: string;
  repositoryCoordinate: string;
  token: {
    scope: { owner: string; community: string };
    workspaceGeneration: number;
    identityGeneration: number;
  };
  label: string;
  workspaceMode: "git" | "folder" | string;
};

export type WikiSourceContent = {
  content: string;
  startLine: number;
  endLine: number;
};

export function chooseWikiSourceRoot(
  repositoryCoordinate: string,
): Promise<WikiSourceGrant | null> {
  return invokeTauri<WikiSourceGrant | null>("wiki_choose_source_root", {
    repositoryCoordinate,
  });
}

export function listWikiSourceGrants(): Promise<WikiSourceGrant[]> {
  return invokeTauri<WikiSourceGrant[]>("wiki_source_grants");
}

export function forgetWikiSourceRoot(capabilityId: string): Promise<void> {
  return invokeTauri<void>("wiki_forget_source_root", { capabilityId });
}

export function openWikiSource(
  capabilityId: string,
  page: RelayEvent,
  referenceIndex: number,
): Promise<WikiSourceContent> {
  return invokeTauri<WikiSourceContent>("wiki_open_verified_source", {
    capabilityId,
    page,
    referenceIndex,
  });
}
