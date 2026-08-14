import { Button } from "@/shared/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/shared/ui/card";
import type {
  UserInputAnswers,
  UserInputEvent,
} from "@/features/channels/lib/userInput";

import { invokeGovernor } from "./governorStore";
import { setCanvasTooling, getCanvasTooling } from "./governorClient";

export function isOriginApprovalRequest(item: UserInputEvent): boolean {
  const values = item.request.questions.flatMap((q) =>
    q.options.map((option) => option.value),
  );
  return (
    values.includes("allow_once") &&
    values.includes("allow_domain") &&
    values.includes("deny")
  );
}

export function OriginApprovalCard({
  item,
  sending,
  onSubmit,
  onSkip,
}: {
  item: UserInputEvent;
  sending?: boolean;
  onSubmit: (item: UserInputEvent, answers: UserInputAnswers) => Promise<void>;
  onSkip: (item: UserInputEvent) => Promise<void>;
}) {
  const origin =
    item.request.questions[0]?.header ||
    item.request.questions[0]?.question ||
    "";
  const questionId = item.request.questions[0]?.id ?? "origin";
  const agent = item.request.engine || "Agent";

  const choose = async (value: "allow_once" | "allow_domain" | "deny") => {
    await invokeGovernor("agent_control_origin_decision", {
      input: {
        channelId: item.request.channel_id,
        origin,
        decision: value,
      },
    }).catch(() => undefined);
    if (value === "allow_domain") {
      const current = await getCanvasTooling(item.request.channel_id).catch(
        () => null,
      );
      const allowlist = [...(current?.browserAllowlist ?? []), origin];
      await setCanvasTooling(item.request.channel_id, {
        ...current,
        browserAllowlist: allowlist,
      }).catch(() => undefined);
    }
    if (value === "deny") {
      await onSkip(item);
      return;
    }
    await onSubmit(item, { [questionId]: value });
  };

  return (
    <Card
      className="border-primary/30 shadow-lg"
      data-testid={`origin-approval-card-${item.event.id}`}
    >
      <CardHeader className="pb-3">
        <CardTitle className="text-base">
          {agent} wants to open an external site
        </CardTitle>
        <CardDescription className="font-mono text-sm text-foreground">
          {origin}
        </CardDescription>
        {item.request.message ? (
          <CardDescription className="text-sm">
            Reason: {item.request.message}
          </CardDescription>
        ) : null}
      </CardHeader>
      <CardContent />
      <CardFooter className="flex flex-wrap gap-2">
        <Button
          data-testid="origin-allow-once"
          disabled={sending}
          size="sm"
          type="button"
          onClick={() => void choose("allow_once")}
        >
          Allow once
        </Button>
        <Button
          data-testid="origin-allow-domain"
          disabled={sending}
          size="sm"
          type="button"
          variant="secondary"
          onClick={() => void choose("allow_domain")}
        >
          Allow domain
        </Button>
        <Button
          data-testid="origin-deny"
          disabled={sending}
          size="sm"
          type="button"
          variant="ghost"
          onClick={() => void choose("deny")}
        >
          Deny
        </Button>
      </CardFooter>
    </Card>
  );
}

export function PendingOriginPrompt({
  channelId,
  origin,
  agentName,
}: {
  channelId: string;
  origin: string;
  agentName?: string | null;
}) {
  const choose = async (value: "allow_once" | "allow_domain" | "deny") => {
    await invokeGovernor("agent_control_origin_decision", {
      input: { channelId, origin, decision: value },
    }).catch(() => undefined);
    if (value === "allow_domain") {
      const current = await getCanvasTooling(channelId).catch(() => null);
      const allowlist = [...(current?.browserAllowlist ?? []), origin];
      await setCanvasTooling(channelId, {
        ...current,
        browserAllowlist: allowlist,
      }).catch(() => undefined);
    }
  };
  const agent = agentName || "Agent";
  return (
    <Card
      className="mx-3 mt-2 border-primary/30 shadow-lg"
      data-testid="origin-approval-card"
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => event.stopPropagation()}
    >
      <CardHeader className="pb-3">
        <CardTitle className="text-base">
          {agent} wants to open an external site
        </CardTitle>
        <CardDescription className="font-mono text-sm text-foreground">
          {origin}
        </CardDescription>
      </CardHeader>
      <CardContent />
      <CardFooter className="flex flex-wrap gap-2">
        <Button
          data-testid="origin-allow-once"
          size="sm"
          type="button"
          onClick={() => void choose("allow_once")}
        >
          Allow once
        </Button>
        <Button
          data-testid="origin-allow-domain"
          size="sm"
          type="button"
          variant="secondary"
          onClick={() => void choose("allow_domain")}
        >
          Allow domain
        </Button>
        <Button
          data-testid="origin-deny"
          size="sm"
          type="button"
          variant="ghost"
          onClick={() => void choose("deny")}
        >
          Deny
        </Button>
      </CardFooter>
    </Card>
  );
}
