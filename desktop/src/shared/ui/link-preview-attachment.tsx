import { setToolPaneTab } from "@/features/tool-pane/toolPaneStore";
import type { MouseEvent } from "react";

import { setThreadViewMode } from "@/features/channels/lib/threadViewModePreference";
import { parseForgePullRequestUrl } from "@/features/messages/lib/parseForgePullRequestUrl";
import { setThreadForgeHubSubject } from "@/features/messages/lib/threadForgeHubSubjectStore";
import { getThreadForgeViewContext } from "@/features/messages/lib/threadForgeViewContextStore";
import type { ResolvedLinkPreview } from "@/shared/lib/useResolvedLinkPreviews";
import { useLinkPreviewStyle } from "@/shared/lib/linkPreviewStylePreference";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { useMediaProxyPort } from "@/shared/lib/useMediaProxyPort";
import { CompactLinkPreviewAttachment } from "@/shared/ui/compact-link-preview-attachment";
import {
  type LinkPreviewImageLightboxComponent,
  RichLinkPreviewAttachment,
} from "@/shared/ui/rich-link-preview-attachment";

export function LinkPreviewAttachment({
  className,
  ImageLightbox,
  onOpen,
  onRemove,
  preview,
  showControls,
}: {
  className?: string;
  ImageLightbox: LinkPreviewImageLightboxComponent;
  onOpen?: () => void;
  onRemove?: () => void;
  preview: ResolvedLinkPreview;
  showControls?: boolean;
}) {
  useMediaProxyPort();
  const renderedPreview = {
    ...preview,
    faviconDataUrl: preview.faviconDataUrl
      ? rewriteRelayUrl(preview.faviconDataUrl)
      : null,
    imageDataUrl: preview.imageDataUrl
      ? rewriteRelayUrl(preview.imageDataUrl)
      : null,
  };
  const style = useLinkPreviewStyle();
  const forgeRef = parseForgePullRequestUrl(preview.href);
  const previewNode =
    style === "rich" ? (
      <RichLinkPreviewAttachment
        className={className}
        ImageLightbox={ImageLightbox}
        onOpen={onOpen}
        onRemove={onRemove}
        preview={renderedPreview}
        showControls={showControls}
      />
    ) : (
      <CompactLinkPreviewAttachment
        className={className}
        onOpen={onOpen}
        onRemove={onRemove}
        preview={renderedPreview}
        showControls={showControls}
      />
    );

  if (!forgeRef) return previewNode;
  const locator = forgeRef;

  function openHub(event: MouseEvent<HTMLButtonElement>) {
    event.preventDefault();
    event.stopPropagation();
    const view = getThreadForgeViewContext();
    setThreadForgeHubSubject({
      kind: "pr",
      owner: locator.owner,
      name: locator.name,
      number: locator.number,
      channelId: view?.channelId ?? null,
      rootEventId: view?.rootEventId ?? null,
      source: "url",
    });
    setToolPaneTab("pr");
    setThreadViewMode("focus");
  }

  return (
    <div className="flex flex-col items-start gap-1">
      {previewNode}
      <button
        className="rounded-md px-2 py-0.5 text-2xs font-medium text-muted-foreground hover:bg-muted hover:text-foreground"
        data-testid="open-pr-hub"
        onClick={openHub}
        type="button"
      >
        Open PR hub — checks & review
      </button>
    </div>
  );
}
