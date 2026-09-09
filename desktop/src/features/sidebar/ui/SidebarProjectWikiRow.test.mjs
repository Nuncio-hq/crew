import assert from "node:assert/strict";
import test from "node:test";
import { SidebarProjectWikiRow } from "./SidebarProjectWikiRow.tsx";
const owner = "a".repeat(64);
const other = "b".repeat(64);
const git = `30617:${owner}:same`;
const folder = `30617:${other}:same`;

test("Wiki row click preserves a full primary coordinate across mixed repositories and owners", () => {
  for (const primary of [git, folder, `30617:${owner}:missing`]) {
    const calls = [];
    const project = {
      id: `30621:${owner}:project`,
      projectAddress: `30621:${owner}:project`,
      name: "Mixed",
      primaryRepositoryAddress: primary,
      repositoryAddresses: [git, folder],
      repositories: [
        { repoAddress: git, workspaceMode: "git" },
        { repoAddress: folder, workspaceMode: "folder" },
      ],
    };
    const row = SidebarProjectWikiRow({
      project,
      active: false,
      onOpen: (...args) => calls.push(args),
    });
    row.props.children.props.onClick();
    assert.deepEqual(calls, [
      [project.id, { tab: "wiki", repositoryAddress: primary }],
    ]);
  }
});
test("Wiki row selects a single member but leaves an unselected multi-repository Project for the selector", () => {
  for (const members of [[git], [git, folder], []]) {
    const calls = [];
    const project = {
      id: `30621:${owner}:project`,
      name: "Project",
      primaryRepositoryAddress: null,
      repositoryAddresses: members,
    };
    SidebarProjectWikiRow({
      project,
      active: false,
      onOpen: (...args) => calls.push(args),
    }).props.children.props.onClick();
    assert.equal(
      calls[0][1].repositoryAddress,
      members.length === 1 ? git : undefined,
    );
  }
});
