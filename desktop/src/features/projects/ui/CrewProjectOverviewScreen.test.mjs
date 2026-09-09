import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test, { afterEach } from "node:test";
afterEach(() => {
  delete globalThis.__OVERVIEW_QUERY__;
  delete globalThis.__OVERVIEW_CHANNELS__;
  delete globalThis.__OVERVIEW_CHANNEL_QUERY__;
  delete globalThis.__OVERVIEW_NAV__;
});
const stubs = new Map([
  [
    "@/app/navigation/useAppNavigation",
    "export const useAppNavigation = () => globalThis.__OVERVIEW_NAV__;",
  ],
  [
    "@/features/channels/hooks",
    "export const useChannelsQuery = () => globalThis.__OVERVIEW_CHANNEL_QUERY__ ?? ({data: globalThis.__OVERVIEW_CHANNELS__});",
  ],
  [
    "@/features/projects/hooks",
    "export const useProjectQuery = () => globalThis.__OVERVIEW_QUERY__; export const projectsQueryKey = ['projects'];",
  ],
]);
registerHooks({
  resolve(specifier, context, next) {
    return stubs.has(specifier)
      ? { shortCircuit: true, url: `overview-stub:${specifier}` }
      : next(specifier, context);
  },
  load(url, context, next) {
    return url.startsWith("overview-stub:")
      ? {
          shortCircuit: true,
          format: "module",
          source: stubs.get(url.slice(14)),
        }
      : next(url, context);
  },
});
const { CrewProjectOverviewScreen } = await import(
  "./CrewProjectOverviewScreen.tsx"
);
test("screen uses query projections and preserves exact navigation targets", () => {
  const calls = [];
  const projectId = `30621:${"a".repeat(64)}:project`;
  const repositoryId = `30617:${"b".repeat(64)}:same`;
  const project = { id: projectId };
  const channels = [{ id: "bound" }];
  globalThis.__OVERVIEW_QUERY__ = { data: project };
  globalThis.__OVERVIEW_CHANNELS__ = channels;
  globalThis.__OVERVIEW_CHANNEL_QUERY__ = null;
  globalThis.__OVERVIEW_NAV__ = Object.fromEntries(
    ["goProject", "goChannel", "goProjects"].map((name) => [
      name,
      (...args) => calls.push([name, ...args]),
    ]),
  );
  const result = CrewProjectOverviewScreen({ projectId, repositoryId });
  assert.equal(result.props.project, project);
  assert.equal(result.props.channels, channels);
  assert.equal(result.props.channelAction.props.project, project);
  assert.equal(result.props.channelAction.props.channels, channels);
  assert.deepEqual(calls, []);
  result.props.onOpenWiki();
  result.props.onOpenChannel("bound");
  result.props.workspaceManagement(repositoryId).props.onClick();
  result.props.onOpenProjects();
  assert.deepEqual(calls, [
    ["goProject", projectId, { repositoryId, tab: "wiki" }],
    ["goChannel", "bound"],
    ["goProject", projectId, { repositoryId, tab: "overview" }],
    ["goProjects"],
  ]);
});

test("channel query failure retains cards and exposes a working retry", () => {
  globalThis.__OVERVIEW_QUERY__ = { data: { id: "project" } };
  globalThis.__OVERVIEW_CHANNELS__ = [];
  globalThis.__OVERVIEW_NAV__ = {
    goProject: () => {},
    goChannel: () => {},
    goProjects: () => {},
  };
  let retries = 0;
  const channels = [{ id: "cached" }];
  globalThis.__OVERVIEW_CHANNEL_QUERY__ = {
    data: channels,
    isError: true,
    refetch: () => {
      retries++;
    },
  };
  const result = CrewProjectOverviewScreen({ projectId: "project" });
  assert.equal(result.props.channels, channels);
  const status = result.props.channelStatus;
  assert.ok(status, "failed channel inventory must expose recovery");
  const children = status.props.children;
  assert.match(children[0], /Could not load/);
  children[1].props.onClick();
  assert.equal(retries, 1);
  globalThis.__OVERVIEW_CHANNEL_QUERY__ = { isPending: true };
  const loading = CrewProjectOverviewScreen({ projectId: "project" });
  assert.match(loading.props.channelStatus.props.children, /Loading/);
});
