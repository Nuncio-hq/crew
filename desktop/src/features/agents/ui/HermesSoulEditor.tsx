import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  readHermesProfileSoul,
  writeHermesProfileSoul,
} from "@/shared/api/hermesProfiles";
import { Button } from "@/shared/ui/button";
import { Textarea } from "@/shared/ui/textarea";

const soulQueryKey = (name: string) => ["hermes-profile-soul", name] as const;

export function HermesSoulEditor({
  profileName,
  disabled = false,
}: {
  profileName: string;
  disabled?: boolean;
}) {
  const queryClient = useQueryClient();
  const queryKey = React.useMemo(
    () => soulQueryKey(profileName),
    [profileName],
  );
  const query = useQuery({
    queryKey,
    queryFn: () => readHermesProfileSoul(profileName),
    enabled: profileName.trim().length > 0,
    refetchOnMount: "always",
  });
  const [content, setContent] = React.useState<string | null>(null);
  const [savedContent, setSavedContent] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const hasLocalEdits = React.useRef(false);

  React.useEffect(() => {
    if (hasLocalEdits.current) return;
    if (query.data?.status === "ok") {
      setContent(query.data.content);
      setSavedContent(query.data.content);
      setError(null);
    } else if (query.data?.status === "missing") {
      setContent("");
      setSavedContent("");
      setError(null);
    }
  }, [query.data]);

  const mutation = useMutation({
    mutationFn: () => writeHermesProfileSoul(profileName, content ?? ""),
    onSuccess: (result) => {
      if (result.status !== "ok") {
        setError(result.message);
        return;
      }
      hasLocalEdits.current = false;
      setContent(result.content);
      setSavedContent(result.content);
      void queryClient.invalidateQueries({ queryKey });
    },
    onError: (cause) => {
      setError(
        cause instanceof Error ? cause.message : "Unable to save persona",
      );
    },
  });

  if (query.isLoading || content === null) {
    if (
      query.data &&
      query.data.status !== "ok" &&
      query.data.status !== "missing"
    ) {
      return (
        <div className="space-y-1.5" data-testid="hermes-soul-error">
          <p className="text-sm font-medium text-foreground">Profile persona</p>
          <p className="text-sm text-destructive">{query.data.message}</p>
        </div>
      );
    }
    return (
      <p className="text-sm text-muted-foreground">Reading profile persona…</p>
    );
  }

  const dirty = content !== savedContent;
  const missing = query.data?.status === "missing";
  return (
    <div
      className="space-y-2 rounded-md border border-border p-3"
      data-testid="hermes-soul-editor"
    >
      <div>
        <p className="text-sm font-medium text-foreground">Profile persona</p>
        <p className="text-sm text-muted-foreground">
          This is the profile’s persona and is shared everywhere the profile
          runs. It applies on the next fresh ACP session; <code>!rotate</code>{" "}
          forces one.
        </p>
        {missing ? (
          <p className="text-sm text-muted-foreground">
            This profile has no persona file yet. Saving will create
            <code> SOUL.md</code>.
          </p>
        ) : null}
      </div>
      <Textarea
        aria-label="Hermes profile persona"
        disabled={disabled || mutation.isPending}
        onChange={(event) => {
          hasLocalEdits.current = true;
          setContent(event.target.value);
        }}
        value={content}
      />
      {error ? <p className="text-sm text-destructive">{error}</p> : null}
      <Button
        disabled={!dirty || disabled || mutation.isPending}
        onClick={() => {
          setError(null);
          mutation.mutate();
        }}
        type="button"
      >
        {mutation.isPending ? "Saving…" : "Save profile persona"}
      </Button>
    </div>
  );
}
