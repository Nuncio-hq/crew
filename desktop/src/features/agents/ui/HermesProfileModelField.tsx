import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  readHermesProfileModel,
  writeHermesProfileModel,
  type HermesProfileConfigResult,
} from "@/shared/api/hermesProfiles";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import {
  crewMayMutateHermesProfile,
  hermesHomeProfileEditInHermesCopy,
} from "../lib/hermesProfileBinding";
import { ProfileOwnedModelRow } from "./HermesProfileBindingFields";

const profileModelQueryKey = (name: string) =>
  ["hermes-profile-model", name] as const;

function okResult(
  result: HermesProfileConfigResult | undefined,
): result is Extract<HermesProfileConfigResult, { status: "ok" }> {
  return result?.status === "ok";
}

export function HermesProfileModelField({
  profileName,
  disabled = false,
}: {
  profileName: string;
  disabled?: boolean;
}) {
  const queryClient = useQueryClient();
  const queryKey = React.useMemo(
    () => profileModelQueryKey(profileName),
    [profileName],
  );
  const query = useQuery({
    queryKey,
    queryFn: () => readHermesProfileModel(profileName),
    enabled:
      profileName.trim().length > 0 && crewMayMutateHermesProfile(profileName),
    refetchOnMount: "always",
  });
  const [provider, setProvider] = React.useState("");
  const [model, setModel] = React.useState("");
  const [error, setError] = React.useState<string | null>(null);
  const [notice, setNotice] = React.useState<string | null>(null);
  const hasLocalEdits = React.useRef(false);

  React.useEffect(() => {
    if (hasLocalEdits.current) return;
    if (!okResult(query.data)) return;
    setProvider(query.data.provider ?? "");
    setModel(query.data.model ?? "");
    setError(null);
  }, [query.data]);

  const mutation = useMutation({
    mutationFn: () =>
      writeHermesProfileModel(
        profileName,
        provider.trim() || undefined,
        model.trim() || undefined,
      ),
    onSuccess: (result) => {
      if (result.status !== "ok") {
        setError(result.message);
        return;
      }
      hasLocalEdits.current = false;
      setProvider(result.provider ?? "");
      setModel(result.model ?? "");
      setNotice("Saved. The new model applies on the next fresh ACP session.");
      void queryClient.invalidateQueries({ queryKey });
    },
    onError: (cause) => {
      setError(cause instanceof Error ? cause.message : "Unable to save model");
    },
  });

  if (!crewMayMutateHermesProfile(profileName)) {
    return (
      <div className="space-y-1.5" data-testid="hermes-home-profile-readonly">
        <p className="text-sm font-medium text-foreground">Profile model</p>
        <p className="text-sm text-muted-foreground">
          Model and provider for the personal default profile are owned by
          Hermes. {hermesHomeProfileEditInHermesCopy()}
        </p>
      </div>
    );
  }

  if (
    query.data?.status === "binary_missing" ||
    query.data?.status === "does_not_exist" ||
    query.data?.status === "failed"
  ) {
    return <ProfileOwnedModelRow profileName={profileName} />;
  }
  if (query.isLoading || !okResult(query.data)) {
    return (
      <div className="space-y-1.5" data-testid="hermes-profile-model-loading">
        <p className="text-sm font-medium text-foreground">Profile model</p>
        <p className="text-sm text-muted-foreground">
          Reading profile settings…
        </p>
      </div>
    );
  }

  const unset = !provider.trim() && !model.trim();
  const invalid = !provider.trim() || !model.trim();
  return (
    <div className="space-y-3" data-testid="hermes-profile-model-field">
      <div>
        <p className="text-sm font-medium text-foreground">Profile model</p>
        <p className="text-sm text-muted-foreground">
          This model belongs to profile <strong>{profileName}</strong> —
          changing it here changes it everywhere the profile runs, not just in
          Crew. It applies on the next fresh ACP session; <code>!rotate</code>{" "}
          forces one.
        </p>
      </div>
      {unset ? (
        <p className="text-sm text-muted-foreground">
          This profile has no model set yet.
        </p>
      ) : null}
      <div className="grid gap-2 sm:grid-cols-2">
        <Input
          aria-label="Hermes profile provider"
          disabled={disabled || mutation.isPending}
          onChange={(event) => {
            hasLocalEdits.current = true;
            setProvider(event.target.value);
          }}
          placeholder="Provider"
          value={provider}
        />
        <Input
          aria-label="Hermes profile model"
          disabled={disabled || mutation.isPending}
          onChange={(event) => {
            hasLocalEdits.current = true;
            setModel(event.target.value);
          }}
          placeholder="Model ID"
          value={model}
        />
      </div>
      {error ? (
        <p
          className="text-sm text-destructive"
          data-testid="hermes-profile-model-error"
        >
          {error}
        </p>
      ) : null}
      {notice ? (
        <p className="text-sm text-muted-foreground">{notice}</p>
      ) : null}
      <Button
        disabled={disabled || mutation.isPending || invalid}
        onClick={() => {
          setError(null);
          setNotice(null);
          mutation.mutate();
        }}
        type="button"
      >
        {mutation.isPending ? "Saving…" : "Save profile model"}
      </Button>
    </div>
  );
}
