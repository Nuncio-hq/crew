import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

globalThis.__NAV_REACT__ = React;
const stubs = new Map([
  ["@/shared/features", "export const FeatureGate = ({children}) => children;"],
  [
    "@protected-feature-components",
    "export const ProtectedBestieSidebarEntry = () => null;",
  ],
  [
    "@/features/search/ui/TopbarSearch",
    "export const TopbarSearch = () => null;",
  ],
  [
    "@/shared/ui/sidebar",
    "const React=globalThis.__NAV_REACT__; export const SidebarHeader='nav', SidebarMenu='ul', SidebarMenuBadge='span', SidebarMenuItem='li'; export const SidebarMenuButton=({isActive,...props})=>React.createElement('button',props);",
  ],
  [
    "@/shared/ui/dropdown-menu",
    "export const DropdownMenu='div', DropdownMenuContent='div', DropdownMenuItem='button', DropdownMenuSeparator='hr', DropdownMenuTrigger=({children})=>children;",
  ],
]);
registerHooks({
  resolve(specifier, context, nextResolve) {
    return stubs.has(specifier)
      ? { shortCircuit: true, url: `nav-stub:${specifier}` }
      : nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    return url.startsWith("nav-stub:")
      ? {
          shortCircuit: true,
          format: "module",
          source: stubs.get(url.slice(9)),
        }
      : nextLoad(url, context);
  },
});
const { AppSidebarPrimaryMenu } = await import("./AppSidebarPinnedHeader.tsx");
const { WorkspaceNavigationMenu } = await import(
  "./WorkspaceNavigationMenu.tsx"
);

test("primary navigation contains Inbox, Agents and Workflows without global Wiki or Pulse", () => {
  const html = renderToStaticMarkup(
    React.createElement(AppSidebarPrimaryMenu, {
      homeBadgeCount: 0,
      selectedView: "home",
    }),
  );
  assert.ok(html.indexOf(">Inbox<") < html.indexOf(">Agents<"));
  assert.ok(html.indexOf(">Agents<") < html.indexOf(">Workflows<"));
  assert.doesNotMatch(html, />Wiki<|>Pulse<|>Channels</);
});
test("workspace menu retains channels, Company Wiki and enabled Pulse independently of Projects", () => {
  const html = renderToStaticMarkup(
    React.createElement(WorkspaceNavigationMenu, { name: "Workspace" }),
  );
  assert.match(html, /Browse channels/);
  assert.match(html, /Company Wiki/);
  assert.match(html, /Pulse/);
  assert.match(html, /Open workspace menu/);
});
