import * as React from "react";

import { SettingsSectionHeader } from "@/features/settings/ui/SettingsSectionHeader";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";

import { formatBytes, simDelete, simErase, simKeep } from "./governorClient";
import { setGovernorPolicy, useGovernorStatus } from "./governorStore";
import { DEFAULT_GOVERNOR_POLICY, type GovernorPolicy } from "./types";

export function DevicesPreviewSettings() {
  const status = useGovernorStatus();
  const [policy, setPolicy] = React.useState<GovernorPolicy>(status.policy);
  React.useEffect(() => {
    setPolicy(status.policy);
  }, [status.policy]);

  return (
    <section data-testid="settings-devices-preview">
      <SettingsSectionHeader
        description="Caps, idle timers, and Crew-booted simulators. The Resource Governor decides; nothing is reclaimed silently."
        title="Devices & Preview"
      />
      <div className="space-y-6">
        <PolicyTable
          onChange={setPolicy}
          onSave={() => {
            void setGovernorPolicy(policy);
          }}
          policy={policy}
        />
        <DeviceList />
        <PruneSection />
      </div>
    </section>
  );
}

function PolicyTable({
  onChange,
  onSave,
  policy,
}: {
  onChange: (next: GovernorPolicy) => void;
  onSave: () => void;
  policy: GovernorPolicy;
}) {
  const rows: Array<{
    key: keyof GovernorPolicy;
    label: string;
    userSetting: boolean;
    unit: string;
  }> = [
    {
      key: "maxBootedSims",
      label: "Concurrent booted sims",
      userSetting: true,
      unit: "",
    },
    {
      key: "maxMirrorStreams",
      label: "Active mirror streams",
      userSetting: true,
      unit: "",
    },
    {
      key: "simIdleShutdownMs",
      label: "Sim idle → shutdown",
      userSetting: true,
      unit: "min",
    },
    {
      key: "streamPauseHiddenMs",
      label: "Stream pause on hidden pane",
      userSetting: false,
      unit: "s",
    },
    {
      key: "hiddenWebviewCap",
      label: "Hidden webviews kept alive",
      userSetting: true,
      unit: "",
    },
    {
      key: "hiddenWebviewTtlMs",
      label: "Destroy hidden webview after",
      userSetting: true,
      unit: "min",
    },
    {
      key: "maxDevServers",
      label: "Concurrent dev servers",
      userSetting: true,
      unit: "",
    },
    {
      key: "devServerIdleMs",
      label: "Dev server idle → stop",
      userSetting: true,
      unit: "min",
    },
    {
      key: "pruneUnusedMs",
      label: "Unused device prune after",
      userSetting: true,
      unit: "days",
    },
  ];
  return (
    <div data-testid="devices-policy-table">
      <h3 className="mb-2 text-sm font-medium">Policy</h3>
      <div className="divide-y divide-border/60 rounded-lg border border-border/60">
        {rows.map((row) => {
          const raw = policy[row.key];
          const display = displayPolicy(row.key, raw, row.unit);
          const isDefault =
            policy[row.key] === DEFAULT_GOVERNOR_POLICY[row.key];
          return (
            <div
              className="flex items-center gap-3 px-3 py-2 text-sm"
              data-testid={`policy-row-${row.key}`}
              key={row.key}
            >
              <span className="flex-1">{row.label}</span>
              {isDefault ? (
                <span className="text-2xs text-muted-foreground">default</span>
              ) : null}
              {row.userSetting ? (
                <Input
                  className="h-7 w-20 text-right text-2xs"
                  disabled={!row.userSetting}
                  onChange={(event) => {
                    const next = Number(event.target.value);
                    if (!Number.isFinite(next)) return;
                    onChange({
                      ...policy,
                      [row.key]: storePolicy(next, row.unit),
                    });
                  }}
                  value={display}
                />
              ) : (
                <span className="w-20 text-right text-2xs text-muted-foreground">
                  {display}
                  {row.unit ? ` ${row.unit}` : ""}
                </span>
              )}
            </div>
          );
        })}
      </div>
      <Button
        className="mt-3"
        data-testid="devices-policy-save"
        onClick={onSave}
        size="sm"
        type="button"
      >
        Save policy
      </Button>
    </div>
  );
}

function DeviceList() {
  const status = useGovernorStatus();
  return (
    <div data-testid="devices-list">
      <h3 className="mb-2 text-sm font-medium">Devices</h3>
      {status.sims.length === 0 ? (
        <p className="text-2xs text-muted-foreground">
          No Crew devices on this machine.
        </p>
      ) : (
        <ul className="space-y-2">
          {status.sims.map((sim) => (
            <li
              className="rounded-lg border border-border/60 px-3 py-2 text-sm"
              data-testid={`device-row-${sim.channelId}`}
              key={sim.channelId}
            >
              <div className="flex items-center gap-2">
                <span className="font-medium">{sim.deviceName}</span>
                <span className="text-2xs text-muted-foreground">
                  {sim.deviceType} · {sim.runtime} · {sim.lifecycle}
                  {sim.foreign ? " · foreign (read-only)" : ""}
                </span>
                <span className="ml-auto text-2xs">
                  {formatBytes(sim.diskBytes)}
                </span>
              </div>
              {sim.foreign ? null : (
                <div className="mt-2 flex gap-2">
                  <Button
                    data-testid={`device-erase-${sim.channelId}`}
                    onClick={() => void simErase(sim.channelId)}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    Erase
                  </Button>
                  <Button
                    data-testid={`device-delete-${sim.channelId}`}
                    onClick={() => void simDelete(sim.channelId)}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    Delete
                  </Button>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function PruneSection() {
  const status = useGovernorStatus();
  if (status.pruneCandidates.length === 0) {
    return (
      <div data-testid="devices-prune">
        <h3 className="mb-2 text-sm font-medium">Prune unused</h3>
        <p className="text-2xs text-muted-foreground">
          Devices unused for more than 30 days will be listed here. Confirm
          before delete.
        </p>
      </div>
    );
  }
  return (
    <div data-testid="devices-prune">
      <h3 className="mb-2 text-sm font-medium">Prune unused</h3>
      {status.pruneCandidates.map((sim) => (
        <div
          className="mb-2 flex items-center gap-2 rounded-lg border border-amber-500/40 px-3 py-2 text-sm"
          key={sim.channelId}
        >
          <span>{sim.deviceName} unused — confirm delete?</span>
          <Button
            className="ml-auto"
            data-testid={`device-prune-keep-${sim.channelId}`}
            onClick={() => void simKeep(sim.channelId)}
            size="sm"
            type="button"
            variant="outline"
          >
            Keep
          </Button>
          <Button
            data-testid={`device-prune-delete-${sim.channelId}`}
            onClick={() => void simDelete(sim.channelId)}
            size="sm"
            type="button"
            variant="outline"
          >
            Delete
          </Button>
        </div>
      ))}
    </div>
  );
}

function displayPolicy(
  _key: keyof GovernorPolicy,
  raw: number,
  unit: string,
): number {
  if (unit === "min") return Math.round(raw / 60_000);
  if (unit === "s") return Math.round(raw / 1000);
  if (unit === "days") return Math.round(raw / (24 * 60 * 60_000));
  return raw;
}

function storePolicy(display: number, unit: string): number {
  if (unit === "min") return display * 60_000;
  if (unit === "s") return display * 1000;
  if (unit === "days") return display * 24 * 60 * 60_000;
  return display;
}
