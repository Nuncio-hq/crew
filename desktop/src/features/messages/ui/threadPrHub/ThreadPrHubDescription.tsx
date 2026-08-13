import { Markdown } from "@/shared/ui/markdown";

export function ThreadPrHubDescription({ body }: { body: string }) {
  if (!body.trim()) {
    return (
      <p
        className="text-sm text-muted-foreground"
        data-testid="thread-pr-hub-description"
      >
        No description.
      </p>
    );
  }
  return (
    <div className="text-sm" data-testid="thread-pr-hub-description">
      <Markdown content={body} />
    </div>
  );
}
