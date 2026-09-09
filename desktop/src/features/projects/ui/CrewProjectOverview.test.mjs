import assert from "node:assert/strict";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { CrewProjectOverview } from "./CrewProjectOverview.tsx";
const owner = "a".repeat(64);
const other = "b".repeat(64);
const repositories = [owner, other].map((pubkey) => ({
  id: `${pubkey}:same`,
  repoAddress: `30617:${pubkey}:same`,
  name: "Same name",
  localWorkspacePath: `/Users/${pubkey.slice(0, 1)}/Shared Folder`,
  localWorkspaceStatus: "linked",
  workspaceMode: "folder",
}));
const project = {
  id: `30621:${owner}:project`,
  name: "Project",
  description: "",
  repositories,
  projectChannelId: "home",
  relatedChannelIds: ["home", "shared", "shared"],
};
function tree(overrides = {}) {
  return CrewProjectOverview({
    project,
    channels: [{ id: "home", name: "Main", description: "Home stream" }],
    channelAction: null,
    workspaceAction: null,
    workspaceManagement: () => null,
    onOpenChannel: () => {},
    onOpenWiki: () => {},
    onOpenProjects: () => {},
    ...overrides,
  });
}
function elements(element) {
  if (!React.isValidElement(element)) return [];
  return [
    element,
    ...React.Children.toArray(element.props.children).flatMap(elements),
  ];
}
test("overview dispatches the bound channel ID and Wiki callbacks without navigation side effects on render", () => {
  const calls = [];
  const result = tree({
    onOpenChannel: (id) => calls.push(id),
    onOpenWiki: () => calls.push("wiki"),
  });
  assert.deepEqual(calls, []);
  const nodes = elements(result);
  const rows = nodes.filter((el) =>
    el.props["data-testid"]?.startsWith("project-overview-channel-"),
  );
  assert.equal(rows.length, 2);
  rows[1].props.onClick();
  const wiki = nodes.find((el) =>
    React.Children.toArray(el.props.children).includes("Open Wiki"),
  );
  wiki.props.onClick();
  assert.deepEqual(calls, ["shared", "wiki"]);
});
test("same-name repositories retain exact coordinates and honest unverified path state", () => {
  const managed = [];
  const html = renderToStaticMarkup(
    tree({
      workspaceManagement: (address) => {
        managed.push(address);
        return null;
      },
    }),
  );
  assert.deepEqual(
    managed,
    repositories.map((repo) => repo.repoAddress),
  );
  for (const repo of repositories) {
    assert.ok(html.includes(repo.repoAddress));
    assert.ok(html.includes(repo.localWorkspacePath));
  }
  assert.match(html, /access on this device not verified/);
  assert.doesNotMatch(html, /online|Linked · sample/);
});
test("zero repositories retains the supplied folder action and channel navigation", () => {
  const html = renderToStaticMarkup(
    tree({
      project: { ...project, repositories: [] },
      workspaceAction: React.createElement(
        "button",
        { type: "button" },
        "Link folder",
      ),
    }),
  );
  assert.match(html, /No workspace linked yet/);
  assert.match(html, /Link folder/);
  assert.match(html, /project-overview-channel-home/);
});
