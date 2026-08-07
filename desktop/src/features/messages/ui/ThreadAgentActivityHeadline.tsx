import {
  useRotatingActivityHeadline,
  type ConversationActivityHeadlineSelection,
} from "@/features/messages/ui/conversationActivityHeadline";
import { cn } from "@/shared/lib/cn";

/**
 * Muted mono activity line shown after a running thread agent chip.
 * Rotates every ~3s with a fade; `prefers-reduced-motion` keeps the latest.
 *
 * Responsive (container queries on the thread row):
 * - L: inline after the chip
 * - M and below (≤659.9px): wraps to its own full-width line
 * - XS (≤419.9px): hidden (chip tooltip still carries `latest`)
 */
export function ThreadAgentActivityHeadline({
  selection,
}: {
  selection: ConversationActivityHeadlineSelection | null;
}) {
  const visible = useRotatingActivityHeadline(
    selection?.headlines ?? [],
    selection?.latest ?? null,
  );
  if (!visible) return null;

  return (
    <span
      aria-hidden
      className={cn(
        "ml-1 min-w-0 max-w-56 truncate font-mono text-2xs font-normal text-muted-foreground/80",
        // M tier and below: own full-width line under the chip row.
        "[@container(max-width:659.9px)]:ml-0 [@container(max-width:659.9px)]:mt-0.5 [@container(max-width:659.9px)]:basis-full [@container(max-width:659.9px)]:max-w-none",
        // XS: hide; chip tooltip still has the latest headline.
        "[@container(max-width:419.9px)]:hidden",
      )}
      data-testid="thread-agent-activity-headline"
      title={selection?.latest ?? visible}
    >
      <span
        className="inline-block animate-in fade-in-0 duration-300 ease-out motion-reduce:animate-none"
        key={visible}
      >
        {visible}
      </span>
    </span>
  );
}
