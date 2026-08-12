import { useEffect, useState } from "react";
import { toast } from "sonner";

import {
  getGlobalAgentConfig,
  setGlobalAgentConfig,
} from "@/shared/api/tauriGlobalAgentConfig";
import type { GlobalAgentConfig } from "@/shared/api/types";
import { Input } from "@/shared/ui/input";
import { SettingsOptionGroup, SettingsOptionRow } from "./SettingsOptionGroup";

const DEFAULT_COMPACTION_THRESHOLD = 3;
const DEFAULT_TURN_THRESHOLD = 100;

export function HandoverSummarizerSettingsCard() {
  const [config, setConfig] = useState<GlobalAgentConfig | null>(null);
  const [modelDraft, setModelDraft] = useState("");
  const [compactionDraft, setCompactionDraft] = useState(
    String(DEFAULT_COMPACTION_THRESHOLD),
  );
  const [turnDraft, setTurnDraft] = useState(String(DEFAULT_TURN_THRESHOLD));
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void getGlobalAgentConfig()
      .then((loaded) => {
        setConfig(loaded);
        setModelDraft(loaded.handover_summarizer_model ?? "");
        setCompactionDraft(
          String(
            loaded.compaction_aging_threshold ?? DEFAULT_COMPACTION_THRESHOLD,
          ),
        );
        setTurnDraft(
          String(loaded.turn_aging_threshold ?? DEFAULT_TURN_THRESHOLD),
        );
      })
      .catch((error) => {
        toast.error(
          error instanceof Error ? error.message : String(error),
        );
      });
  }, []);

  async function persist(next: GlobalAgentConfig) {
    setSaving(true);
    try {
      const result = await setGlobalAgentConfig(next);
      setConfig(result.config);
      toast.success(
        result.restarted_count > 0
          ? `Saved. Restarted ${result.restarted_count} agent(s).`
          : "Saved.",
      );
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  }

  if (!config) {
    return null;
  }

  return (
    <SettingsOptionGroup
      data-testid="handover-summarizer-settings"
      title="Session aging & handover"
    >
      <SettingsOptionRow>
        <div className="min-w-0 flex-1 space-y-1.5">
          <label
            className="text-sm font-medium"
            htmlFor="handover-summarizer-model"
          >
            Handover summarizer model
          </label>
          <p
            className="text-sm font-normal text-muted-foreground/70"
            data-settings-subcopy
          >
            Cheap model used for owner-triggered guided handover notes. Same
            for every agent — not a per-agent setting.
          </p>
          <Input
            data-testid="handover-summarizer-model-input"
            disabled={saving}
            id="handover-summarizer-model"
            onBlur={() => {
              const next = {
                ...config,
                handover_summarizer_model: modelDraft.trim() || null,
              };
              if (
                next.handover_summarizer_model ===
                config.handover_summarizer_model
              ) {
                return;
              }
              void persist(next);
            }}
            onChange={(event) => setModelDraft(event.target.value)}
            placeholder="e.g. gpt-4o-mini"
            value={modelDraft}
          />
        </div>
      </SettingsOptionRow>

      <SettingsOptionRow>
        <div className="min-w-0 flex-1 space-y-1.5">
          <label
            className="text-sm font-medium"
            htmlFor="compaction-aging-threshold"
          >
            Compaction aging threshold (1–10)
          </label>
          <Input
            data-testid="compaction-aging-threshold-input"
            disabled={saving}
            id="compaction-aging-threshold"
            inputMode="numeric"
            onBlur={() => {
              const parsed = Number.parseInt(compactionDraft, 10);
              const clamped = Number.isFinite(parsed)
                ? Math.min(10, Math.max(1, parsed))
                : DEFAULT_COMPACTION_THRESHOLD;
              setCompactionDraft(String(clamped));
              const next = {
                ...config,
                compaction_aging_threshold: clamped,
              };
              if (next.compaction_aging_threshold === config.compaction_aging_threshold) {
                return;
              }
              void persist(next);
            }}
            onChange={(event) => setCompactionDraft(event.target.value)}
            value={compactionDraft}
          />
        </div>
      </SettingsOptionRow>

      <SettingsOptionRow>
        <div className="min-w-0 flex-1 space-y-1.5">
          <label className="text-sm font-medium" htmlFor="turn-aging-threshold">
            Turn-count aging safety net
          </label>
          <Input
            data-testid="turn-aging-threshold-input"
            disabled={saving}
            id="turn-aging-threshold"
            inputMode="numeric"
            onBlur={() => {
              const parsed = Number.parseInt(turnDraft, 10);
              const value =
                Number.isFinite(parsed) && parsed > 0
                  ? parsed
                  : DEFAULT_TURN_THRESHOLD;
              setTurnDraft(String(value));
              const next = {
                ...config,
                turn_aging_threshold: value,
              };
              if (next.turn_aging_threshold === config.turn_aging_threshold) {
                return;
              }
              void persist(next);
            }}
            onChange={(event) => setTurnDraft(event.target.value)}
            value={turnDraft}
          />
        </div>
      </SettingsOptionRow>
    </SettingsOptionGroup>
  );
}
