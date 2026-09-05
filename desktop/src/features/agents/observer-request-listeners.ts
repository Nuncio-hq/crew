import {
  parseAgentManagementRequest,
  type AgentManagementRequest,
} from "./agentManagement";
import {
  parseProjectChannelRequest,
  type ProjectChannelRequest,
} from "@/features/projects/projectChannelRequest";

const agentManagementListeners = new Set<
  (agentPubkey: string, request: AgentManagementRequest) => void
>();
const projectChannelRequestListeners = new Set<
  (agentPubkey: string, request: ProjectChannelRequest) => void
>();

export function subscribeAgentManagementRequests(
  listener: (agentPubkey: string, request: AgentManagementRequest) => void,
) {
  agentManagementListeners.add(listener);
  return () => {
    agentManagementListeners.delete(listener);
  };
}

export function subscribeProjectChannelRequests(
  listener: (agentPubkey: string, request: ProjectChannelRequest) => void,
) {
  projectChannelRequestListeners.add(listener);
  return () => {
    projectChannelRequestListeners.delete(listener);
  };
}

/** Dispatch requests only after the observer envelope has passed authorization. */
export function dispatchObserverRequests(
  agentPubkey: string,
  payload: unknown,
) {
  const managementRequest = parseAgentManagementRequest(payload);
  if (managementRequest) {
    for (const listener of agentManagementListeners) {
      listener(agentPubkey, managementRequest);
    }
  }
  const projectChannelRequest = parseProjectChannelRequest(payload);
  if (projectChannelRequest) {
    for (const listener of projectChannelRequestListeners) {
      listener(agentPubkey, projectChannelRequest);
    }
  }
}

/** Cleared by the observer store at every community boundary. */
export function resetObserverRequestListeners() {
  agentManagementListeners.clear();
  projectChannelRequestListeners.clear();
}
