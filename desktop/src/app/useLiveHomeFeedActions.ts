import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { remindersQueryKey } from "@/features/reminders/hooks";
import { relayClient } from "@/shared/api/relayClient";
import {
  KIND_APPROVAL_REQUEST,
  KIND_APPROVAL_GRANT,
  KIND_APPROVAL_DENY,
  KIND_EVENT_REMINDER,
  KIND_AGENT_USER_INPUT_REQUESTED,
  KIND_AGENT_USER_INPUT_ANSWER,
  KIND_AGENT_USER_INPUT_RESOLVED,
  KIND_AGENT_RECEIPT,
  KIND_REACTION,
} from "@/shared/constants/kinds";
import { getThreadReference } from "@/features/messages/lib/threading";
import {
  beginExhaustiveApprovalProjection,
  endExhaustiveApprovalProjection,
  ingestApprovalRequestEvent,
  isExhaustiveApprovalProjectionCurrent,
  resolveApprovalRequestEvent,
  settlePendingApprovalResolutions,
} from "@/features/agents/needsYouStore";
import {
  beginExhaustiveUserInputProjection,
  endExhaustiveUserInputProjection,
  isUserInputAttentionProjectionUnavailable,
  markUserInputAttentionProjectionUnavailable,
  projectAuthorizedUserInputEvent,
  reconcileAuthorizedUserInputRequests,
} from "@/features/agents/userInputAttentionProjection";
import { useCurrentOwnedAgentPubkeys } from "@/features/home/useOwnedAgentPubkeys";
import { buildChannelUserInputFilter } from "@/shared/api/relayChannelFilters";
import {
  beginExhaustiveAgentReceiptProjection,
  endExhaustiveAgentReceiptProjection,
  ingestAgentReceiptEvent,
  ingestAgentReceiptReviewEvent,
  isAgentReceiptProjectionUnavailable,
  markAgentReceiptProjectionUnavailable,
} from "@/features/agents/agentReceiptStore";
import {
  createHydrationRetryController,
  enumerateDurableActionEvents,
  isPermanentHydrationError,
  mergeDurableActionEvents,
} from "@/features/agents/durableActionHydration";
import type { RelayEvent } from "@/shared/api/types";
import { fetchRelayEventAncestry } from "@/features/agents/receiptParentLookup";
import { failApprovalSubscriptions } from "./failApprovalSubscriptions";
import {
  activeFamilyStateIsCurrent,
  createDurableProjectionFamilyCounters,
  eventBelongsToActiveProjection,
  type DurableProjectionFamily,
} from "@/app/durableProjectionFamily";

const LIVE_HOME_FEED_RETRY_BASE_MS = 1_000;
const LIVE_HOME_FEED_RETRY_MAX_MS = 30_000;
const DURABLE_ACTION_PAGE_SIZE = 500;
const RECEIPT_PARENT_BATCH_SIZE = 100;
const MAX_DURABLE_HYDRATION_BUFFER = 5_000;

class DurableProjectionHydrationError extends Error {
  constructor(
    readonly family: DurableProjectionFamily,
    readonly originalError: unknown,
  ) {
    super(
      originalError instanceof Error
        ? originalError.message
        : String(originalError),
    );
    this.name = "DurableProjectionHydrationError";
  }
}

function causalParentId(event: RelayEvent): string | null {
  const thread = getThreadReference(event.tags);
  return thread.parentId ?? thread.rootId;
}

export function useLiveHomeFeedActions(
  pubkey: string | undefined,
  onHomeFeedEvent: () => void,
  channelIds: readonly string[] = [],
) {
  const queryClient = useQueryClient();
  const ownedAgentPubkeys = useCurrentOwnedAgentPubkeys(pubkey);
  // Re-subscribe only when channel membership actually changes.
  const channelIdsKey = channelIds.join(",");

  const handleLiveHomeFeedEvent = React.useEffectEvent(() => {
    onHomeFeedEvent();
  });
  const handleLiveReminderEvent = React.useEffectEvent(
    (normalizedPubkey: string) => {
      onHomeFeedEvent();
      void queryClient.invalidateQueries({
        queryKey: remindersQueryKey(normalizedPubkey),
      });
    },
  );

  React.useEffect(() => {
    const normalizedPubkey = pubkey?.trim().toLowerCase() ?? "";
    if (!normalizedPubkey) {
      return;
    }
    const subscribedChannelIds =
      channelIdsKey.length > 0 ? channelIdsKey.split(",") : [];

    let isCancelled = false;
    const familyDisposers: Record<
      DurableProjectionFamily,
      Array<() => Promise<void>>
    > = {
      receipt: [],
      userInput: [],
    };
    type AuxiliarySubscriptionKey =
      | "approvalRequest"
      | "approvalTerminal"
      | "reminder";
    const auxiliaryDisposers: Record<
      AuxiliarySubscriptionKey,
      Array<() => Promise<void>>
    > = { approvalRequest: [], approvalTerminal: [], reminder: [] };
    let retryTimer: ReturnType<typeof globalThis.setTimeout> | null = null;
    let retryAttempt = 0;
    const since = Math.floor(Date.now() / 1_000);
    const familyHydrationReady: Record<DurableProjectionFamily, boolean> = {
      receipt: false,
      userInput: false,
    };
    const terminalFamilies = new Set<DurableProjectionFamily>();
    const durableProjectionGeneration = createDurableProjectionFamilyCounters();
    const durableBufferOverflowGeneration =
      createDurableProjectionFamilyCounters();
    const bufferedDurableEvents = new Map<string, RelayEvent>();
    const projectionOwnerByFamily: Record<
      DurableProjectionFamily,
      number | null
    > = { receipt: null, userInput: null };
    const beginFamilyProjection = (family: DurableProjectionFamily) => {
      const owner =
        family === "userInput"
          ? beginExhaustiveUserInputProjection()
          : beginExhaustiveAgentReceiptProjection(
              normalizedPubkey,
              ownedAgentPubkeys,
            );
      projectionOwnerByFamily[family] = owner;
      return owner;
    };
    const ensureFamilyProjectionOwner = (family: DurableProjectionFamily) =>
      projectionOwnerByFamily[family] ?? beginFamilyProjection(family);
    const familyForEvent = (
      event: RelayEvent,
    ): DurableProjectionFamily | null => {
      if (
        [
          KIND_AGENT_USER_INPUT_REQUESTED,
          KIND_AGENT_USER_INPUT_ANSWER,
          KIND_AGENT_USER_INPUT_RESOLVED,
        ].includes(event.kind)
      ) {
        return "userInput";
      }
      if ([KIND_AGENT_RECEIPT, KIND_REACTION].includes(event.kind)) {
        return "receipt";
      }
      return null;
    };
    const isCoarselyAuthorizedDurableEvent = (event: RelayEvent) => {
      const author = event.pubkey.toLowerCase();
      return event.kind === KIND_REACTION
        ? author === normalizedPubkey
        : ownedAgentPubkeys.has(author);
    };
    const bufferDurableEvent = (event: RelayEvent) => {
      const family = familyForEvent(event);
      if (!family || terminalFamilies.has(family)) return false;
      if (!isCoarselyAuthorizedDurableEvent(event)) return false;
      if (
        !bufferedDurableEvents.has(event.id) &&
        [...bufferedDurableEvents.values()].filter(
          (candidate) => familyForEvent(candidate) === family,
        ).length >= MAX_DURABLE_HYDRATION_BUFFER
      ) {
        for (const [eventId, candidate] of bufferedDurableEvents) {
          if (familyForEvent(candidate) === family)
            bufferedDurableEvents.delete(eventId);
        }
        durableBufferOverflowGeneration[family] += 1;
        durableProjectionGeneration[family] += 1;
        familyHydrationReady[family] = false;
        const owner = ensureFamilyProjectionOwner(family);
        if (family === "userInput")
          markUserInputAttentionProjectionUnavailable(owner);
        else markAgentReceiptProjectionUnavailable(owner);
        void hydrationRetry.run();
      }
      bufferedDurableEvents.set(event.id, event);
      return true;
    };

    const disposeAll = (currentDisposers: Array<() => Promise<void>>) => {
      void Promise.allSettled(currentDisposers.map((dispose) => dispose()));
    };
    const handleUserInputEvent = (
      event: RelayEvent,
      fallbackChannelId: string,
      knownParents: ReadonlyMap<string, RelayEvent> | undefined,
      projectionOwner: number,
    ) => {
      const parentId = causalParentId(event);
      projectAuthorizedUserInputEvent(
        event,
        fallbackChannelId,
        normalizedPubkey,
        ownedAgentPubkeys,
        knownParents?.get(parentId ?? ""),
        knownParents,
        projectionOwner,
      );
    };
    const fetchCausalParents = async (events: readonly RelayEvent[]) => {
      const parentIds = [
        ...new Set(
          events
            .map((event) => causalParentId(event))
            .filter((eventId): eventId is string => Boolean(eventId)),
        ),
      ];
      return fetchRelayEventAncestry(
        (filter) => relayClient.fetchEvents(filter),
        parentIds,
        RECEIPT_PARENT_BATCH_SIZE,
      );
    };
    const handleReceiptEvent = async (
      event: RelayEvent,
      knownParents: ReadonlyMap<string, RelayEvent> | undefined,
      shouldCommit: () => boolean,
      projectionOwner: number,
    ) => {
      if (event.kind === KIND_AGENT_RECEIPT) {
        const parentId = causalParentId(event);
        const causalEvents =
          knownParents ?? (await fetchCausalParents([event]));
        const parent = causalEvents.get(parentId ?? "");
        if (!isCancelled && shouldCommit())
          ingestAgentReceiptEvent(event, parent, causalEvents, projectionOwner);
      } else if (event.kind === KIND_REACTION) {
        if (!shouldCommit()) return;
        ingestAgentReceiptReviewEvent(
          event,
          normalizedPubkey,
          ownedAgentPubkeys,
          projectionOwner,
        );
        if (isAgentReceiptProjectionUnavailable()) {
          throw new Error("agent receipt review projection capacity exceeded");
        }
      }
    };
    const hydrateDurableActions = async () => {
      if (subscribedChannelIds.length === 0) return;
      let userInputActive =
        familyDisposers.userInput.length > 0 &&
        !familyHydrationReady.userInput &&
        !terminalFamilies.has("userInput");
      let receiptActive =
        familyDisposers.receipt.length > 0 &&
        !familyHydrationReady.receipt &&
        !terminalFamilies.has("receipt");
      if (!userInputActive && !receiptActive) return;
      if (userInputActive) durableProjectionGeneration.userInput += 1;
      if (receiptActive) durableProjectionGeneration.receipt += 1;
      const projectionGenerationAtStart = { ...durableProjectionGeneration };
      const overflowGenerationAtStart = {
        ...durableBufferOverflowGeneration,
      };
      const activeFamilies = {
        receipt: receiptActive,
        userInput: userInputActive,
      };
      const ownsHydrationGeneration = () =>
        activeFamilyStateIsCurrent(
          durableProjectionGeneration,
          projectionGenerationAtStart,
          activeFamilies,
        );
      const activeFamilyOverflowed = () =>
        !activeFamilyStateIsCurrent(
          durableBufferOverflowGeneration,
          overflowGenerationAtStart,
          activeFamilies,
        );
      if (userInputActive) familyHydrationReady.userInput = false;
      if (receiptActive) familyHydrationReady.receipt = false;
      const enumerateFamily = async <T>(
        family: DurableProjectionFamily,
        enumerate: () => Promise<T>,
      ) => {
        try {
          return await enumerate();
        } catch (error) {
          throw new DurableProjectionHydrationError(family, error);
        }
      };
      const [userInputResult, receiptResult] = await Promise.allSettled([
        userInputActive
          ? enumerateFamily("userInput", () =>
              enumerateDurableActionEvents(
                (filter) => relayClient.fetchEvents(filter),
                {
                  kinds: [
                    KIND_AGENT_USER_INPUT_REQUESTED,
                    KIND_AGENT_USER_INPUT_ANSWER,
                    KIND_AGENT_USER_INPUT_RESOLVED,
                  ],
                  "#h": subscribedChannelIds,
                },
                DURABLE_ACTION_PAGE_SIZE,
              ),
            )
          : Promise.resolve([]),
        receiptActive
          ? enumerateFamily("receipt", async () => {
              const [receipts, reviews] = await Promise.all([
                enumerateDurableActionEvents(
                  (filter) => relayClient.fetchEvents(filter),
                  {
                    kinds: [KIND_AGENT_RECEIPT],
                    "#h": subscribedChannelIds,
                  },
                  DURABLE_ACTION_PAGE_SIZE,
                ),
                enumerateDurableActionEvents(
                  (filter) => relayClient.fetchEvents(filter),
                  {
                    authors: [normalizedPubkey],
                    kinds: [KIND_REACTION],
                    "#h": subscribedChannelIds,
                  },
                  DURABLE_ACTION_PAGE_SIZE,
                ),
              ]);
              return { receipts, reviews };
            })
          : Promise.resolve({ receipts: [], reviews: [] }),
      ]);
      let hydrationRetryError: unknown = null;
      const userInputEvents =
        userInputResult.status === "fulfilled" ? userInputResult.value : [];
      if (userInputResult.status === "rejected") {
        userInputActive = false;
        activeFamilies.userInput = false;
        hydrationRetryError = userInputResult.reason;
      }
      const { receipts: receiptEvents, reviews: reviewEvents } =
        receiptResult.status === "fulfilled"
          ? receiptResult.value
          : { receipts: [], reviews: [] };
      if (receiptResult.status === "rejected") {
        receiptActive = false;
        activeFamilies.receipt = false;
        hydrationRetryError ??= receiptResult.reason;
      }
      if (!userInputActive && !receiptActive) throw hydrationRetryError;
      if (isCancelled || !ownsHydrationGeneration()) return;
      let merged = mergeDurableActionEvents(
        userInputEvents,
        receiptEvents,
        reviewEvents,
        [],
      );
      let projectionStarted = false;
      let userInputProjectionOwner: number | null = null;
      let receiptProjectionOwner: number | null = null;
      const isActiveHydrationEvent = (event: RelayEvent) =>
        eventBelongsToActiveProjection(event, familyForEvent, activeFamilies);
      for (;;) {
        const overlap: RelayEvent[] = [];
        for (const [eventId, event] of bufferedDurableEvents) {
          if (!isActiveHydrationEvent(event)) continue;
          overlap.push(event);
          bufferedDurableEvents.delete(eventId);
        }
        merged = mergeDurableActionEvents(
          merged.userInputEvents,
          merged.receiptEvents,
          merged.reviewEvents,
          overlap,
        );
        const causalParents = await fetchCausalParents([
          ...merged.userInputEvents.filter(
            (event) => event.kind === KIND_AGENT_USER_INPUT_REQUESTED,
          ),
          ...merged.receiptEvents,
        ]);
        if (isCancelled || !ownsHydrationGeneration()) return;
        if (activeFamilyOverflowed()) {
          throw new Error(
            "Durable live-overlap capacity exceeded before projection commit",
          );
        }
        if (!projectionStarted) {
          if (userInputActive)
            userInputProjectionOwner = beginFamilyProjection("userInput");
          if (receiptActive)
            receiptProjectionOwner = beginFamilyProjection("receipt");
          projectionStarted = true;
        }
        const userInputOwner = userInputProjectionOwner;
        const receiptOwner = receiptProjectionOwner;
        if (
          (userInputActive && userInputOwner === null) ||
          (receiptActive && receiptOwner === null)
        ) {
          throw new Error("durable projection owner was not established");
        }
        for (const event of merged.userInputEvents) {
          if (userInputOwner === null) break;
          handleUserInputEvent(
            event,
            event.tags.find((tag) => tag[0] === "h")?.[1] ?? "",
            causalParents,
            userInputOwner,
          );
        }
        if (userInputOwner !== null)
          reconcileAuthorizedUserInputRequests(
            normalizedPubkey,
            ownedAgentPubkeys,
            userInputOwner,
          );
        // Receipts establish authority before reactions are projected, even if
        // relay pages or same-second ids arrive in the opposite order.
        for (const event of merged.receiptEvents) {
          if (receiptOwner === null) break;
          await handleReceiptEvent(
            event,
            causalParents,
            () => true,
            receiptOwner,
          );
        }
        for (const event of merged.reviewEvents) {
          if (receiptOwner === null) break;
          await handleReceiptEvent(event, undefined, () => true, receiptOwner);
        }
        if (isCancelled || !ownsHydrationGeneration()) {
          if (userInputOwner !== null && userInputActive)
            markUserInputAttentionProjectionUnavailable(userInputOwner);
          if (receiptOwner !== null && receiptActive)
            markAgentReceiptProjectionUnavailable(receiptOwner);
          return;
        }
        if (activeFamilyOverflowed()) {
          throw new Error(
            "Durable live-overlap capacity exceeded; exhaustive recovery required",
          );
        }
        if (![...bufferedDurableEvents.values()].some(isActiveHydrationEvent)) {
          if (userInputOwner !== null && userInputActive) {
            if (endExhaustiveUserInputProjection(userInputOwner))
              familyHydrationReady.userInput = true;
          }
          if (receiptOwner !== null && receiptActive) {
            if (endExhaustiveAgentReceiptProjection(receiptOwner))
              familyHydrationReady.receipt = true;
          }
          break;
        }
      }
      handleLiveHomeFeedEvent();
      if (hydrationRetryError) throw hydrationRetryError;
    };
    const hydrateApprovals = async () => {
      const projectionGeneration = beginExhaustiveApprovalProjection();
      let hydrationCompleted = false;
      let projectionReady = false;
      let ownedProjection = false;
      try {
        const [approvalRequestEvents, approvalTerminalEvents] =
          await Promise.all([
            enumerateDurableActionEvents(
              (filter) => relayClient.fetchEvents(filter),
              {
                kinds: [KIND_APPROVAL_REQUEST],
                "#p": [normalizedPubkey],
              },
              DURABLE_ACTION_PAGE_SIZE,
            ),
            enumerateDurableActionEvents(
              (filter) => relayClient.fetchEvents(filter),
              {
                authors: [normalizedPubkey],
                kinds: [KIND_APPROVAL_GRANT, KIND_APPROVAL_DENY],
              },
              DURABLE_ACTION_PAGE_SIZE,
            ),
          ]);
        if (
          isCancelled ||
          !isExhaustiveApprovalProjectionCurrent(projectionGeneration)
        )
          return;
        for (const event of approvalRequestEvents)
          ingestApprovalRequestEvent(event, projectionGeneration);
        for (const event of approvalTerminalEvents) {
          await resolveApprovalRequestEvent(event, projectionGeneration);
        }
        hydrationCompleted =
          await settlePendingApprovalResolutions(projectionGeneration);
      } finally {
        ownedProjection =
          isExhaustiveApprovalProjectionCurrent(projectionGeneration);
        if (ownedProjection)
          projectionReady = endExhaustiveApprovalProjection(
            projectionGeneration,
            hydrationCompleted,
          );
      }
      if (!ownedProjection) return;
      if (!projectionReady) {
        throw new Error(
          "approval projection overflowed during exhaustive hydration",
        );
      }
      handleLiveHomeFeedEvent();
    };
    const markFamilyPermanent = (
      family: DurableProjectionFamily,
      error: unknown,
    ) => {
      if (isCancelled || terminalFamilies.has(family)) return;
      terminalFamilies.add(family);
      familyHydrationReady[family] = false;
      durableProjectionGeneration[family] += 1;
      for (const [eventId, event] of bufferedDurableEvents) {
        if (familyForEvent(event) === family)
          bufferedDurableEvents.delete(eventId);
      }
      if (family === "userInput") {
        const owner = ensureFamilyProjectionOwner("userInput");
        markUserInputAttentionProjectionUnavailable(owner);
        endExhaustiveUserInputProjection(owner);
      } else {
        const owner = ensureFamilyProjectionOwner("receipt");
        markAgentReceiptProjectionUnavailable(owner);
        endExhaustiveAgentReceiptProjection(owner);
      }
      const currentDisposers = familyDisposers[family];
      familyDisposers[family] = [];
      disposeAll(currentDisposers);
      console.error(
        `Durable ${family} projection is unavailable until relay policy/configuration changes`,
        error,
      );
      if (
        (["receipt", "userInput"] as const).some(
          (candidate) =>
            !terminalFamilies.has(candidate) &&
            familyDisposers[candidate].length > 0,
        )
      ) {
        void hydrationRetry.run();
      }
      handleLiveHomeFeedEvent();
    };
    const hydrationRetry = createHydrationRetryController({
      hydrate: hydrateDurableActions,
      onError: (error) =>
        console.error(
          "Failed to hydrate durable agent attention events",
          error,
        ),
      onPermanentError: (error) => {
        if (error instanceof DurableProjectionHydrationError) {
          markFamilyPermanent(error.family, error.originalError);
          return;
        }
        for (const family of ["receipt", "userInput"] as const) {
          if (
            !terminalFamilies.has(family) &&
            familyDisposers[family].length > 0
          )
            markFamilyPermanent(family, error);
        }
      },
      retryDelayMs: LIVE_HOME_FEED_RETRY_MAX_MS,
      setTimeoutFn: (callback, delayMs) =>
        globalThis.setTimeout(callback, delayMs),
      clearTimeoutFn: (timer) => globalThis.clearTimeout(timer),
      shouldRetry: (error) =>
        !isPermanentHydrationError(
          error instanceof DurableProjectionHydrationError
            ? error.originalError
            : error,
        ),
    });
    const approvalHydrationRetry = createHydrationRetryController({
      hydrate: hydrateApprovals,
      onError: (error) =>
        console.error("Failed to hydrate approval events", error),
      onPermanentError: (error) =>
        console.error(
          "Approval projection is unavailable until relay policy/configuration changes",
          error,
        ),
      retryDelayMs: LIVE_HOME_FEED_RETRY_MAX_MS,
      setTimeoutFn: (callback, delayMs) =>
        globalThis.setTimeout(callback, delayMs),
      clearTimeoutFn: (timer) => globalThis.clearTimeout(timer),
    });
    const scheduleRetry = () => {
      if (isCancelled || retryTimer !== null) {
        return;
      }

      const delay = Math.min(
        LIVE_HOME_FEED_RETRY_MAX_MS,
        LIVE_HOME_FEED_RETRY_BASE_MS * 2 ** Math.min(retryAttempt, 5),
      );
      retryAttempt += 1;
      retryTimer = globalThis.setTimeout(() => {
        retryTimer = null;
        void startSubscriptions();
      }, delay);
    };
    const startSubscriptions = () => {
      if (isCancelled) {
        return Promise.resolve();
      }

      const subscribeCritical = (
        family: DurableProjectionFamily,
        filter: Parameters<typeof relayClient.subscribeLive>[0],
        onEvent: Parameters<typeof relayClient.subscribeLive>[1],
      ) =>
        relayClient.subscribeLive(filter, onEvent, (status) => {
          if (status.state === "closed") {
            markFamilyPermanent(family, new Error(status.message));
          }
        });
      const userInputSubscriptions =
        terminalFamilies.has("userInput") ||
        familyDisposers.userInput.length > 0
          ? []
          : subscribedChannelIds.map((channelId) =>
              subscribeCritical(
                "userInput",
                buildChannelUserInputFilter(channelId, 50, since),
                (event) => {
                  if (terminalFamilies.has("userInput")) return;
                  if (!bufferDurableEvent(event)) return;
                  if (!familyHydrationReady.userInput) {
                    return;
                  }
                  const eventGeneration = durableProjectionGeneration.userInput;
                  void fetchCausalParents([event])
                    .then((parents) => {
                      if (
                        isCancelled ||
                        !familyHydrationReady.userInput ||
                        durableProjectionGeneration.userInput !==
                          eventGeneration
                      )
                        return;
                      handleUserInputEvent(
                        event,
                        channelId,
                        parents,
                        ensureFamilyProjectionOwner("userInput"),
                      );
                      if (isUserInputAttentionProjectionUnavailable()) {
                        bufferDurableEvent(event);
                        familyHydrationReady.userInput = false;
                        durableProjectionGeneration.userInput += 1;
                        void hydrationRetry.run();
                        return;
                      }
                      bufferedDurableEvents.delete(event.id);
                      handleLiveHomeFeedEvent();
                    })
                    .catch((error) => {
                      if (
                        isCancelled ||
                        terminalFamilies.has("userInput") ||
                        durableProjectionGeneration.userInput !==
                          eventGeneration
                      )
                        return;
                      bufferDurableEvent(event);
                      familyHydrationReady.userInput = false;
                      durableProjectionGeneration.userInput += 1;
                      console.error(
                        "Failed to validate user-input parent",
                        error,
                      );
                      void hydrationRetry.run();
                    });
                },
              ),
            );
      const receiptSubscriptions =
        terminalFamilies.has("receipt") || familyDisposers.receipt.length > 0
          ? []
          : subscribedChannelIds.flatMap((channelId) => [
              subscribeCritical(
                "receipt",
                {
                  kinds: [KIND_AGENT_RECEIPT],
                  "#h": [channelId],
                  limit: 0,
                  since,
                },
                (event) => {
                  if (terminalFamilies.has("receipt")) return;
                  if (!bufferDurableEvent(event)) return;
                  if (!familyHydrationReady.receipt) {
                    return;
                  }
                  const eventGeneration = durableProjectionGeneration.receipt;
                  const ownsGeneration = () =>
                    familyHydrationReady.receipt &&
                    durableProjectionGeneration.receipt === eventGeneration;
                  void handleReceiptEvent(
                    event,
                    undefined,
                    ownsGeneration,
                    ensureFamilyProjectionOwner("receipt"),
                  )
                    .then(() => {
                      if (!ownsGeneration()) return;
                      bufferedDurableEvents.delete(event.id);
                      handleLiveHomeFeedEvent();
                    })
                    .catch((error) => {
                      if (isCancelled || !ownsGeneration()) return;
                      bufferDurableEvent(event);
                      familyHydrationReady.receipt = false;
                      durableProjectionGeneration.receipt += 1;
                      console.error(
                        "Failed to validate agent receipt parent",
                        error,
                      );
                      void hydrationRetry.run();
                    });
                },
              ),
              subscribeCritical(
                "receipt",
                {
                  authors: [normalizedPubkey],
                  kinds: [KIND_REACTION],
                  "#h": [channelId],
                  limit: 0,
                  since,
                },
                (event) => {
                  if (terminalFamilies.has("receipt")) return;
                  if (!bufferDurableEvent(event)) return;
                  if (!familyHydrationReady.receipt) {
                    return;
                  }
                  const eventGeneration = durableProjectionGeneration.receipt;
                  const ownsGeneration = () =>
                    familyHydrationReady.receipt &&
                    durableProjectionGeneration.receipt === eventGeneration;
                  void handleReceiptEvent(
                    event,
                    undefined,
                    ownsGeneration,
                    ensureFamilyProjectionOwner("receipt"),
                  )
                    .then(() => {
                      if (!ownsGeneration()) return;
                      bufferedDurableEvents.delete(event.id);
                      handleLiveHomeFeedEvent();
                    })
                    .catch(() => {
                      if (isCancelled || !ownsGeneration()) return;
                      bufferDurableEvent(event);
                      familyHydrationReady.receipt = false;
                      durableProjectionGeneration.receipt += 1;
                      void hydrationRetry.run();
                    });
                },
              ),
            ]);

      const familySetups: Array<{
        family: DurableProjectionFamily;
        promises: Array<Promise<() => Promise<void>>>;
      }> = [];
      if (userInputSubscriptions.length > 0)
        familySetups.push({
          family: "userInput",
          promises: userInputSubscriptions,
        });
      if (receiptSubscriptions.length > 0)
        familySetups.push({
          family: "receipt",
          promises: receiptSubscriptions,
        });

      const subscribeAuxiliary = (
        key: AuxiliarySubscriptionKey,
        label: string,
        filter: Parameters<typeof relayClient.subscribeLive>[0],
        onEvent: Parameters<typeof relayClient.subscribeLive>[1],
      ) =>
        relayClient.subscribeLive(filter, onEvent, (status) => {
          if (status.state !== "closed") return;
          console.error(`${label} subscription closed`, status.message);
          if (key === "reminder") {
            disposeAll(auxiliaryDisposers.reminder);
            auxiliaryDisposers.reminder = [];
          } else {
            failApprovalSubscriptions(auxiliaryDisposers);
          }
        });
      const auxiliarySetups = [
        {
          key: "approvalRequest" as const,
          label: "Approval request",
          filter: {
            kinds: [KIND_APPROVAL_REQUEST],
            "#p": [normalizedPubkey],
            limit: 50,
            since,
          },
          onEvent: (event: RelayEvent) => {
            ingestApprovalRequestEvent(event);
            handleLiveHomeFeedEvent();
          },
        },
        {
          key: "approvalTerminal" as const,
          label: "Approval terminal",
          filter: {
            authors: [normalizedPubkey],
            kinds: [KIND_APPROVAL_GRANT, KIND_APPROVAL_DENY],
            limit: 50,
            since,
          },
          onEvent: (event: RelayEvent) => {
            void resolveApprovalRequestEvent(event).finally(
              handleLiveHomeFeedEvent,
            );
          },
        },
        {
          key: "reminder" as const,
          label: "Reminder",
          filter: {
            authors: [normalizedPubkey],
            kinds: [KIND_EVENT_REMINDER],
            limit: 50,
            since,
          },
          onEvent: () => handleLiveReminderEvent(normalizedPubkey),
        },
      ];
      const auxiliarySubscriptions = auxiliarySetups
        .filter(({ key }) => auxiliaryDisposers[key].length === 0)
        .map(({ key, label, filter, onEvent }) =>
          subscribeAuxiliary(key, label, filter, onEvent).then(
            (dispose) => ({ dispose, key, error: null }),
            (error) => ({ dispose: null, key, error }),
          ),
        );

      return Promise.all([
        Promise.all(
          familySetups.map(async ({ family, promises }) => ({
            family,
            results: await Promise.allSettled(promises),
          })),
        ),
        Promise.allSettled(auxiliarySubscriptions),
      ]).then(([familyResults, auxiliaryResults]) => {
        let hydrationNeeded = false;
        let retryNeeded = false;
        let approvalSetupPermanent = false;
        for (const { family, results } of familyResults) {
          const fulfilled = results.flatMap((result) =>
            result.status === "fulfilled" ? [result.value] : [],
          );
          const rejected = results.filter(
            (result) => result.status === "rejected",
          );
          if (isCancelled) {
            disposeAll(fulfilled);
            continue;
          }
          if (terminalFamilies.has(family)) {
            disposeAll(fulfilled);
            continue;
          }
          for (const result of rejected) {
            console.error(
              `Failed to subscribe to ${family} live actions`,
              result.reason,
            );
          }
          const permanent = rejected.find((result) =>
            isPermanentHydrationError(result.reason),
          );
          if (permanent) {
            disposeAll(fulfilled);
            markFamilyPermanent(family, permanent.reason);
          } else if (rejected.length > 0 || fulfilled.length === 0) {
            disposeAll(fulfilled);
            retryNeeded = true;
          } else {
            familyDisposers[family] = fulfilled;
            hydrationNeeded = true;
          }
        }

        for (const result of auxiliaryResults) {
          if (result.status === "rejected") {
            console.error(
              "Auxiliary home-feed subscription unavailable",
              result.reason,
            );
            retryNeeded = true;
            continue;
          }
          const { dispose, error, key } = result.value;
          if (error) {
            console.error(`Auxiliary ${key} subscription unavailable`, error);
            if (!isPermanentHydrationError(error)) retryNeeded = true;
            else if (key !== "reminder") approvalSetupPermanent = true;
            continue;
          }
          if (!dispose) continue;
          if (isCancelled) {
            disposeAll([dispose]);
            continue;
          }
          auxiliaryDisposers[key] = [dispose];
        }
        if (approvalSetupPermanent) {
          failApprovalSubscriptions(auxiliaryDisposers);
        }
        if (
          auxiliaryDisposers.approvalRequest.length > 0 &&
          auxiliaryDisposers.approvalTerminal.length > 0
        ) {
          void approvalHydrationRetry.run();
        }

        if (retryNeeded) scheduleRetry();
        else retryAttempt = 0;
        if (hydrationNeeded) {
          // Install each live overlap before taking its history snapshot.
          void hydrationRetry.run();
        }
      });
    };

    void startSubscriptions();

    return () => {
      isCancelled = true;
      if (retryTimer !== null) {
        globalThis.clearTimeout(retryTimer);
      }
      hydrationRetry.stop();
      approvalHydrationRetry.stop();
      for (const family of ["receipt", "userInput"] as const) {
        const currentDisposers = familyDisposers[family];
        familyDisposers[family] = [];
        disposeAll(currentDisposers);
      }
      for (const key of [
        "approvalRequest",
        "approvalTerminal",
        "reminder",
      ] as const) {
        const currentDisposers = auxiliaryDisposers[key];
        auxiliaryDisposers[key] = [];
        disposeAll(currentDisposers);
      }
    };
  }, [channelIdsKey, ownedAgentPubkeys, pubkey]);
}
