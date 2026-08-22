import { useMemo, useState } from "react";
import { ChevronDown, Moon, Sun, SunMoon } from "lucide-react";
import { useCommunities } from "@/features/communities/useCommunities";
import { Badge } from "@/shared/ui/badge";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { cn } from "@/shared/lib/cn";
import {
  type ChromeThemeName,
  isChromeThemeName,
} from "@/shared/theme/chrome-theme";
import {
  DEFAULT_SYNTAX_THEME,
  formatSyntaxThemeLabel,
  isShikiPaletteName,
} from "@/shared/theme/theme-preference-migration";
import {
  SYNTAX_THEMES,
  type SyntaxThemeName,
} from "@/shared/theme/theme-loader";
import { useTheme } from "@/shared/theme/ThemeProvider";
import { SyntaxHighlightedCode } from "@/shared/ui/markdown/CodeBlock";
import { appearanceCommunityLabel } from "../lib/appearanceScopeCopy";
import {
  ConversationDisplaySettings,
  GlassBackgroundSetting,
  LinkPreviewStyleSetting,
  ThreadLayoutSetting,
} from "./AppearanceSettingsControls";
import { SettingsOptionGroup, SettingsOptionRow } from "./SettingsOptionGroup";
import { SettingsSectionHeader } from "./SettingsSectionHeader";

type ChromeChoice = ChromeThemeName | "system";

const CHROME_OPTIONS = [
  { value: "crew-dark" as const, label: "Crew Dark", Icon: Moon },
  { value: "crew-light" as const, label: "Crew Light", Icon: Sun },
  { value: "system" as const, label: "System", Icon: SunMoon },
] as const;

const SYNTAX_PREVIEW = `function greet(name: string) {
  return \`hello, \${name}\`;
}
`;

const SYNTAX_OPTIONS = (SYNTAX_THEMES as readonly string[]).filter(
  (name): name is SyntaxThemeName => isShikiPaletteName(name),
);

function currentChromeChoice(
  followSystem: boolean,
  selected: string,
): ChromeChoice {
  if (followSystem) return "system";
  return isChromeThemeName(selected) ? selected : "crew-dark";
}

export function AppearanceThemeSettings() {
  const {
    selectedThemeName,
    syntaxThemeName,
    followSystem,
    setTheme,
    setSyntaxTheme,
    setFollowSystem,
  } = useTheme();
  const { activeCommunity, communities } = useCommunities();
  const showCommunityScope = communities.length > 1;
  const communityLabel = appearanceCommunityLabel(activeCommunity?.name);
  const selectedChrome = currentChromeChoice(followSystem, selectedThemeName);
  const [syntaxMenuOpen, setSyntaxMenuOpen] = useState(false);

  const syntaxLabel = useMemo(
    () => formatSyntaxThemeLabel(syntaxThemeName || DEFAULT_SYNTAX_THEME),
    [syntaxThemeName],
  );

  const handleChromeSelect = (choice: ChromeChoice) => {
    if (choice === "system") {
      setFollowSystem(true);
      return;
    }
    setFollowSystem(false);
    setTheme(choice);
  };

  return (
    <section
      className="flex min-h-0 flex-1 flex-col overflow-y-auto"
      data-testid="settings-theme"
    >
      <SettingsSectionHeader
        title="Appearance"
        description="Chrome is Crew Dark or Crew Light. Code syntax is a separate palette."
      />

      <div className="space-y-12">
        <SettingsOptionGroup
          data-testid="appearance-theme-card"
          headerAction={
            showCommunityScope && activeCommunity ? (
              <Badge
                className="max-w-56 font-medium normal-case tracking-normal"
                data-testid="appearance-community-badge"
                variant="outline"
              >
                <span className="truncate">{communityLabel}</span>
              </Badge>
            ) : null
          }
          title={
            <>
              Theme
              {showCommunityScope ? (
                <span className="ml-1 font-normal text-muted-foreground">
                  (per community)
                </span>
              ) : null}
            </>
          }
        >
          <SettingsOptionRow data-testid="appearance-chrome-row">
            <div className="min-w-0">
              <p className="text-sm font-medium">Theme</p>
              <p
                className="text-sm font-normal text-muted-foreground/70"
                data-settings-subcopy
              >
                Crew Dark, Crew Light, or follow the system.
              </p>
            </div>
            <fieldset
              className="relative isolate grid h-8 w-[18rem] shrink-0 grid-cols-3 overflow-hidden rounded-md bg-muted/45 p-0.5"
              data-testid="appearance-chrome-control"
            >
              <legend className="sr-only">Theme</legend>
              <div
                aria-hidden="true"
                className="absolute bottom-0.5 left-0.5 top-0.5 z-0 rounded-md bg-background shadow-sm transition-transform duration-[250ms] ease-out motion-reduce:transition-none"
                data-testid="appearance-chrome-indicator"
                style={{
                  transform: `translateX(${CHROME_OPTIONS.findIndex((option) => option.value === selectedChrome) * 100}%)`,
                  width: "calc((100% - 4px) / 3)",
                }}
              />
              {CHROME_OPTIONS.map(({ value, label, Icon }) => (
                <button
                  aria-pressed={selectedChrome === value}
                  className={cn(
                    "relative z-10 flex h-full items-center justify-center gap-1.5 rounded-md bg-transparent px-2 text-xs font-medium transition-colors duration-[250ms] ease-out focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring motion-reduce:transition-none",
                    selectedChrome === value
                      ? "text-foreground"
                      : "text-muted-foreground hover:text-foreground",
                  )}
                  data-testid={`appearance-chrome-${value}`}
                  key={value}
                  onClick={() => handleChromeSelect(value)}
                  type="button"
                >
                  <Icon className="h-3.5 w-3.5" />
                  {label}
                </button>
              ))}
            </fieldset>
          </SettingsOptionRow>

          <SettingsOptionRow
            className="items-start"
            data-testid="appearance-syntax-row"
          >
            <div className="min-w-0">
              <p className="text-sm font-medium">Code syntax</p>
              <p
                className="text-sm font-normal text-muted-foreground/70"
                data-settings-subcopy
              >
                Palette for fenced code blocks. Default is Dark Plus.
              </p>
            </div>
            <DropdownMenu
              modal={false}
              onOpenChange={setSyntaxMenuOpen}
              open={syntaxMenuOpen}
            >
              <DropdownMenuTrigger asChild>
                <Button
                  className="h-7 min-w-40 justify-between gap-1.5 rounded-md border border-border/50 bg-muted/45 px-2.5 text-xs font-medium text-foreground shadow-none hover:bg-muted/70"
                  data-testid="syntax-theme-trigger"
                  size="sm"
                  type="button"
                  variant="ghost"
                >
                  <span className="truncate">{syntaxLabel}</span>
                  <ChevronDown className="h-4 w-4 text-muted-foreground" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent
                align="end"
                className="max-h-80 min-w-64 overflow-y-auto rounded-md"
                data-testid="syntax-theme-menu"
              >
                <DropdownMenuRadioGroup
                  onValueChange={(next) => {
                    setSyntaxTheme(next);
                  }}
                  value={syntaxThemeName}
                >
                  {SYNTAX_OPTIONS.map((name) => (
                    <DropdownMenuRadioItem
                      data-testid={`syntax-theme-${name}`}
                      key={name}
                      value={name}
                    >
                      {formatSyntaxThemeLabel(name)}
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuRadioGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          </SettingsOptionRow>

          <div className="px-4 py-3" data-testid="syntax-theme-preview">
            <p className="mb-2 text-2xs font-medium uppercase tracking-[0.14em] text-muted-foreground">
              Preview
            </p>
            <pre className="max-h-40 overflow-auto rounded-lg border border-border/70 bg-muted/40 px-3 py-2">
              <SyntaxHighlightedCode code={SYNTAX_PREVIEW} language="ts" />
            </pre>
          </div>

          <GlassBackgroundSetting />
        </SettingsOptionGroup>

        <SettingsOptionGroup
          data-testid="appearance-display-card"
          title="Display"
        >
          <ConversationDisplaySettings />
        </SettingsOptionGroup>

        <SettingsOptionGroup
          data-testid="appearance-preferences-card"
          title="Preferences"
        >
          <LinkPreviewStyleSetting />
          <ThreadLayoutSetting />
        </SettingsOptionGroup>
      </div>
    </section>
  );
}
