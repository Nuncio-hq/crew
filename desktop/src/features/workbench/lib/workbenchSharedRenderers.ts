/**
 * Identity re-exports for the Thread Workbench (#186).
 *
 * The workbench is a different layout of the same components the channel
 * view already uses. Importing through this module is the component-reuse
 * contract: tests assert these bindings are the exact channel/session
 * implementations, not forked copies.
 */
export { MessageRow } from "@/features/messages/ui/MessageRow";
export { EvidenceCard } from "@/features/messages/ui/EvidenceCard";
export { ChannelUserInputCard } from "@/features/channels/ui/ChannelUserInputCard";
export { ChannelUserInputStack } from "@/features/channels/ui/ChannelUserInputStack";
export { ProjectThreadGitHubRow } from "@/features/messages/ui/ProjectThreadGitHubRow";
export { ProjectThreadWorkspacePanel } from "@/features/messages/ui/ProjectThreadWorkspacePanel";
export { UnreadDivider } from "@/features/messages/ui/UnreadDivider";
export { SessionAgingBannerSlot } from "@/features/messages/ui/SessionAgingBannerSlot";
export { MessageComposer } from "@/features/messages/ui/MessageComposer";
export { TranscriptActivityItem } from "@/features/agents/ui/activityRenderClasses/TranscriptActivityItem";
export { MessageRowDefaultBody } from "@/features/messages/ui/MessageRowDefaultBody";
