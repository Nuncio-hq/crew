/**
 * Autocomplete combobox for Hermes profile names (feature 0001 Phase 04).
 * Keeps a real text input (id stable for E2E fill) and a popover list of disk profiles.
 *
 * The input is the search/value field, not the Radix trigger. Anchor the
 * popover to the full field and ignore outside-dismiss from that field so
 * focusing or clicking the input cannot close a sliver dropdown.
 */
import { Check, ChevronsUpDown } from "lucide-react";
import * as React from "react";

import { cn } from "@/shared/lib/cn";
import { Input } from "@/shared/ui/input";
import {
  Popover,
  PopoverAnchor,
  PopoverContent,
  PopoverTrigger,
} from "@/shared/ui/popover";
import {
  filterHermesProfileOptions,
  hermesProfileOccupancyLabel,
  type HermesProfileOccupancy,
} from "../lib/hermesProfileBinding";
import {
  PERSONA_FIELD_CONTROL_CLASS,
  PERSONA_FIELD_SHELL_CLASS,
} from "./agentConfigOptions";

export function HermesProfileCombobox({
  value,
  onChange,
  disabled,
  id,
  profiles,
  occupancy,
  listFailed = false,
  listLoading = false,
}: {
  value: string;
  onChange: (next: string) => void;
  disabled?: boolean;
  id: string;
  profiles: readonly string[];
  occupancy: ReadonlyMap<string, HermesProfileOccupancy>;
  listFailed?: boolean;
  listLoading?: boolean;
}) {
  const [open, setOpen] = React.useState(false);
  const [highlightedIndex, setHighlightedIndex] = React.useState(0);
  const fieldRef = React.useRef<HTMLDivElement>(null);

  const filtered = React.useMemo(
    () => filterHermesProfileOptions(profiles, value),
    [profiles, value],
  );

  function selectProfile(name: string) {
    onChange(name);
    setOpen(false);
  }

  function preventDismissFromField(
    event: CustomEvent<{ originalEvent: Event }>,
  ) {
    const target = event.detail.originalEvent.target;
    if (target instanceof Node && fieldRef.current?.contains(target)) {
      event.preventDefault();
    }
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (!open) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        setOpen(true);
      }
      return;
    }

    switch (event.key) {
      case "ArrowDown": {
        event.preventDefault();
        if (filtered.length === 0) return;
        setHighlightedIndex((i) => (i + 1) % filtered.length);
        break;
      }
      case "ArrowUp": {
        event.preventDefault();
        if (filtered.length === 0) return;
        setHighlightedIndex((i) => (i - 1 + filtered.length) % filtered.length);
        break;
      }
      case "Enter": {
        if (filtered[highlightedIndex]) {
          event.preventDefault();
          selectProfile(filtered[highlightedIndex]);
        }
        break;
      }
      case "Escape": {
        event.preventDefault();
        setOpen(false);
        break;
      }
    }
  }

  return (
    <Popover onOpenChange={setOpen} open={open}>
      <PopoverAnchor asChild>
        <div
          className={cn(
            "flex min-h-11 items-center gap-1 px-3",
            PERSONA_FIELD_SHELL_CLASS,
          )}
          ref={fieldRef}
        >
          <Input
            autoCorrect="off"
            className={cn(
              "h-8 px-0 py-0 leading-6",
              PERSONA_FIELD_CONTROL_CLASS,
            )}
            disabled={disabled}
            id={id}
            onChange={(event) => {
              onChange(event.target.value);
              setHighlightedIndex(0);
              if (!open) setOpen(true);
            }}
            onFocus={() => setOpen(true)}
            onKeyDown={handleKeyDown}
            placeholder="Search or type a profile name"
            role="combobox"
            spellCheck={false}
            value={value}
            aria-autocomplete="list"
            aria-expanded={open}
            aria-controls={open ? `${id}-listbox` : undefined}
          />
          <PopoverTrigger asChild>
            <button
              aria-label="Show Hermes profiles"
              className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground disabled:opacity-50"
              data-testid="hermes-profile-combobox-trigger"
              disabled={disabled}
              type="button"
            >
              <ChevronsUpDown className="h-4 w-4" />
            </button>
          </PopoverTrigger>
        </div>
      </PopoverAnchor>
      <PopoverContent
        align="start"
        className="w-(--radix-popover-trigger-width) p-0"
        onCloseAutoFocus={(event) => event.preventDefault()}
        onFocusOutside={preventDismissFromField}
        onInteractOutside={preventDismissFromField}
        onOpenAutoFocus={(event) => event.preventDefault()}
        sideOffset={6}
      >
        <div
          className="max-h-60 overflow-y-auto p-1"
          data-testid="hermes-profile-combobox-list"
          id={`${id}-listbox`}
          role="listbox"
        >
          {listLoading && profiles.length === 0 ? (
            <p className="px-3 py-4 text-center text-xs text-muted-foreground">
              Loading profiles…
            </p>
          ) : listFailed && profiles.length === 0 ? (
            <p
              className="px-3 py-4 text-center text-xs text-muted-foreground"
              data-testid="hermes-profile-list-error"
            >
              Couldn&apos;t load profiles — type a name below.
            </p>
          ) : filtered.length === 0 ? (
            <p className="px-3 py-4 text-center text-xs text-muted-foreground">
              {profiles.length === 0
                ? "No profiles yet — type a name and create one."
                : "No matching profiles."}
            </p>
          ) : (
            filtered.map((name, index) => {
              const occ = occupancy.get(name);
              const selected = name === value.trim();
              const boundOther = occ?.status === "bound";
              return (
                <button
                  className={cn(
                    "flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent hover:text-accent-foreground",
                    selected && "bg-accent/50",
                    index === highlightedIndex &&
                      "bg-accent text-accent-foreground",
                  )}
                  data-profile={name}
                  data-testid="hermes-profile-option"
                  key={name}
                  onClick={() => selectProfile(name)}
                  role="option"
                  type="button"
                  aria-selected={selected}
                >
                  <Check
                    className={cn(
                      "h-4 w-4 shrink-0",
                      selected ? "opacity-100" : "opacity-0",
                    )}
                  />
                  <span className="min-w-0 flex-1 truncate font-medium">
                    {name}
                  </span>
                  <span
                    className={cn(
                      "shrink-0 text-2xs",
                      boundOther
                        ? "text-attention dark:text-attention"
                        : "text-muted-foreground",
                    )}
                    data-testid="hermes-profile-occupancy"
                  >
                    {hermesProfileOccupancyLabel(occ)}
                  </span>
                </button>
              );
            })
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
