import {
  formatFullDateTime,
  formatTime,
  formatTimeWithoutDayPeriod,
} from "@/features/messages/lib/dateFormatters";
import { formatItemTimestamp } from "@/shared/lib/datetime";
import { cn } from "@/shared/lib/cn";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";

export function MessageTimestamp({
  className,
  createdAt,
  hideDayPeriod = false,
}: {
  className?: string;
  createdAt: number;
  hideDayPeriod?: boolean;
}) {
  const displayTime = hideDayPeriod
    ? formatTimeWithoutDayPeriod(formatTime(createdAt))
    : formatItemTimestamp(createdAt, { withTime: true });

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <p
          className={cn(
            "shrink-0 cursor-default whitespace-nowrap text-message-timestamp font-normal tabular-nums text-muted-foreground/55",
            className,
          )}
          data-testid="message-timestamp"
        >
          {displayTime}
        </p>
      </TooltipTrigger>
      <TooltipContent side="top">
        {formatFullDateTime(createdAt)}
      </TooltipContent>
    </Tooltip>
  );
}
