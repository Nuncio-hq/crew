import type * as React from "react";
import type { Components } from "react-markdown";

import { parseChannelLink } from "@/features/messages/lib/channelLink";
import {
  parseMessageLink,
  resolveMessageLinkRenderTarget,
} from "@/features/messages/lib/messageLink";
import { parseEntityLink } from "@/shared/lib/entityLink";

import {
  AuthoredDeepLinkAnchor,
  ChannelDeepLinkAnchor,
  MarkdownChannelDeepLink,
  MarkdownChannelReference,
} from "./ChannelDeepLink";
import { EntityLinkAnchor, renderEntityLinkAnchor } from "./entityLinks";
import { MessageLinkPill } from "./MessageLinkPill";
import { useMarkdownRuntime } from "./runtimeContext";
import type { MarkdownRuntime } from "./types";

type PermalinkRuntime = Pick<
  MarkdownRuntime,
  | "channels"
  | "onOpenChannel"
  | "onOpenEntityLink"
  | "onOpenMessageLink"
  | "relayOrigin"
  | "resolveChannelReferences"
>;

/**
 * Channel, message, and entity permalinks for Markdown `<a>` tags.
 * Returns null so the caller can fall through to ExternalLinkAnchor.
 */
export function tryRenderBuzzPermalinkAnchor({
  children,
  href,
  interactive,
  label,
  props,
  runtime,
}: {
  children: React.ReactNode;
  href: string | undefined;
  interactive: boolean;
  label: string;
  props: React.ComponentPropsWithoutRef<"a">;
  runtime: PermalinkRuntime;
}): React.ReactElement | null {
  const {
    channels,
    onOpenChannel,
    onOpenEntityLink,
    onOpenMessageLink,
    relayOrigin,
    resolveChannelReferences,
  } = runtime;

  if (href && parseChannelLink(href).ok) {
    return (
      <ChannelDeepLinkAnchor {...props} href={href} interactive={interactive}>
        {children}
      </ChannelDeepLinkAnchor>
    );
  }

  if (href) {
    const messageLinkTarget = resolveMessageLinkRenderTarget({
      href,
      label,
    });
    if (messageLinkTarget.kind !== "none") {
      if (messageLinkTarget.kind === "pill") {
        return (
          <MessageLinkPill
            channels={channels}
            interactive={interactive}
            link={messageLinkTarget.link}
            resolveChannelReference={resolveChannelReferences}
            onOpenChannel={onOpenChannel}
            onOpenMessageLink={onOpenMessageLink}
          />
        );
      }
      return (
        <AuthoredDeepLinkAnchor
          channelId={messageLinkTarget.link.channelId}
          href={href}
          interactive={interactive}
          messageLink={messageLinkTarget.link}
        >
          {children}
        </AuthoredDeepLinkAnchor>
      );
    }
    // Malformed message deep links fall through to entity / external handling.
  }

  // `buzz://pr|issue|repo?…` entity links navigate in-app; malformed ones
  // fall through to the default anchor. Authored labels stay ordinary
  // links; bare URLs become compact chips. Wiki `buzz://file` stays inline.
  return renderEntityLinkAnchor({
    children,
    href,
    onOpenEntityLink,
    relayOrigin,
    interactive,
    asChip: label === href,
  });
}

function MarkdownEntityLink({
  children,
  interactive,
}: {
  children?: React.ReactNode;
  interactive: boolean;
}) {
  const { onOpenEntityLink, relayOrigin } = useMarkdownRuntime();
  const href = String(children ?? "");
  if (!parseEntityLink(href).ok) {
    return <span data-entity-link="">{href}</span>;
  }
  return (
    <EntityLinkAnchor
      href={href}
      interactive={interactive}
      onOpenEntityLink={onOpenEntityLink}
      relayOrigin={relayOrigin}
    >
      {href}
    </EntityLinkAnchor>
  );
}

function MarkdownMessageLink({
  children,
  interactive,
}: {
  children?: React.ReactNode;
  interactive: boolean;
}) {
  const {
    channels,
    onOpenChannel,
    onOpenMessageLink,
    resolveChannelReferences,
  } = useMarkdownRuntime();
  const href = String(children ?? "");
  const parsed = parseMessageLink(href);
  if (!parsed.ok) {
    // Malformed link: render the raw URL rather than a misleading pill.
    return <span data-message-link="">{href}</span>;
  }
  return (
    <MessageLinkPill
      channels={channels}
      interactive={interactive}
      link={parsed.value}
      resolveChannelReference={resolveChannelReferences}
      onOpenChannel={onOpenChannel}
      onOpenMessageLink={onOpenMessageLink}
    />
  );
}

/** Custom markdown tags for bare `buzz://` permalinks (remark plugins). */
export function buzzPermalinkComponents(interactive: boolean): Components {
  return {
    "channel-deep-link": ({ children }: { children?: React.ReactNode }) => (
      <MarkdownChannelDeepLink interactive={interactive}>
        {children}
      </MarkdownChannelDeepLink>
    ),
    "channel-link": ({ children }: { children?: React.ReactNode }) => (
      <MarkdownChannelReference interactive={interactive}>
        {children}
      </MarkdownChannelReference>
    ),
    "entity-link": ({ children }: { children?: React.ReactNode }) => (
      <MarkdownEntityLink interactive={interactive}>
        {children}
      </MarkdownEntityLink>
    ),
    "message-link": ({ children }: { children?: React.ReactNode }) => (
      <MarkdownMessageLink interactive={interactive}>
        {children}
      </MarkdownMessageLink>
    ),
  } as Components;
}
