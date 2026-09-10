import { expect, test, type Page } from "@playwright/test";
import { installMockBridge } from "../helpers/bridge";
import { openWorkspaceChannel } from "../helpers/workspaceNavigation";

async function openEditor(page: Page) {
  await installMockBridge(page);
  await page.goto("/");
  await expect(page.getByTestId("app-sidebar")).toBeVisible();
  await page.evaluate(() => {
    const win = window as unknown as {
      __TAURI_INTERNALS__: {
        invoke: (cmd: string, args?: unknown) => Promise<unknown>;
      };
      roleFixture: {
        outcome: string;
        saved: unknown[];
        saveResult: string;
        generation: number;
        deferSave: boolean;
        releaseSave?: () => void;
        currentHead: string;
        canvasHead: string;
        canvasReads: number;
        canvasReadError: boolean;
        saveError: boolean;
      };
    };
    const original = win.__TAURI_INTERNALS__.invoke;
    const owner = "deadbeef".repeat(8),
      one = "a".repeat(64),
      two = "b".repeat(64);
    const token = {
      scope: { owner, community: "http://localhost:3000" },
      workspace_generation: 1,
      identity_generation: 1,
    };
    win.roleFixture = {
      outcome: "canvas_committed_announcement_pending",
      saved: [],
      saveResult: "saved",
      generation: 1,
      deferSave: false,
      currentHead: "new-head",
      canvasHead: "old-head",
      canvasReads: 0,
      canvasReadError: false,
      saveError: false,
    };
    win.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
      const progress = {
        operation_id: "role-save",
        canvas_event_id: "new-head",
        current_event_id: win.roleFixture.currentHead,
        outcome: win.roleFixture.outcome,
        automatic_retry_at: null,
        manual_retry_required: false,
      };
      if (cmd === "owner_operation_scope")
        return { ...token, identity_generation: win.roleFixture.generation };
      if (cmd === "get_canvas") {
        win.roleFixture.canvasReads++;
        if (win.roleFixture.canvasReadError)
          throw new Error("Canvas unavailable");
        return {
          content: "# Working agreement",
          event_id: win.roleFixture.canvasHead,
          definitions: [
            { role_label: "Review", definition: "Inspect patches" },
            { role_label: "Research", definition: "Find sources" },
          ],
          stored_assignments: {
            [one]: "Review",
            [two]: "Review",
            "unknown-key": "Ghost",
          },
          stored_routing: {},
          stored_capabilities: {},
          contact_pubkey: one,
          crew_authority: "owner",
          crew_parse_state: "valid",
          routing: [],
          assignments: [],
          dev_mcp_granted: null,
          crew_parse_error: null,
        };
      }
      if (cmd === "list_relay_agents")
        return [one, two].map((pubkey, i) => ({
          pubkey,
          name: i ? "Morgan" : "Alex",
          agent_type: "codex",
          channel_ids: ["9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50"],
          channels: ["general"],
          capabilities: [],
          status: "online",
        }));
      if (cmd === "list_channel_crew_operations") return { token, value: [] };
      if (cmd === "save_channel_crew_config") {
        win.roleFixture.saved.push(args);
        if (win.roleFixture.saveError)
          throw new Error("Save unavailable before journal creation");
        if (win.roleFixture.deferSave)
          await new Promise<void>((resolve) => {
            win.roleFixture.releaseSave = resolve;
          });
        return {
          token,
          value: {
            result: win.roleFixture.saveResult,
            current_event_id: "other-head",
            progress,
          },
        };
      }
      if (cmd === "get_channel_crew_operation")
        return { token, value: progress };
      return original(cmd, args);
    };
  });
  await openWorkspaceChannel(page, "general");
  await page.getByTestId("channel-management-trigger").click();
  await page.getByTestId("channel-canvas-ingress").click();
  await page.getByTestId("channel-manage-roles").click();
  const dialog = page.getByTestId("channel-roles-dialog");
  return dialog;
}

test("bulk roles retain the draft through partial save and only close after current-head confirmation", async ({
  page,
}) => {
  const dialog = await openEditor(page);
  await expect(dialog.getByRole("textbox", { name: "Role name" })).toHaveCount(
    2,
  );
  await expect(
    dialog.getByRole("checkbox", { name: "Alex", exact: true }),
  ).toBeChecked();
  await expect(
    dialog.getByRole("checkbox", { name: "Morgan", exact: true }),
  ).toBeChecked();
  await dialog
    .getByRole("checkbox", { name: "Alex — move from Review" })
    .check();
  await dialog
    .getByRole("textbox", { name: "Role name" })
    .first()
    .fill("Code review");
  await dialog.getByRole("button", { name: "Save roles", exact: true }).click();
  await expect(
    dialog.getByText(
      "Canvas saved; the working-agreement announcement is still pending.",
    ),
  ).toBeVisible();
  await expect(
    dialog.getByRole("textbox", { name: "Role name" }).first(),
  ).toHaveValue("Code review");
  const saved = await page.evaluate(
    () =>
      (
        window as unknown as {
          roleFixture: {
            saved: {
              draft: {
                assignments: Record<string, string>;
                preserved_assignments: Record<string, string>;
              };
            }[];
          };
        }
      ).roleFixture.saved,
  );
  expect(saved).toHaveLength(1);
  expect(saved[0].draft.assignments["a".repeat(64)]).toBe("Research");
  expect(saved[0].draft.assignments["b".repeat(64)]).toBe("Code review");
  expect(saved[0].draft.preserved_assignments).toEqual({
    "unknown-key": "Ghost",
  });
  await page.evaluate(() => {
    (
      window as unknown as { roleFixture: { outcome: string } }
    ).roleFixture.outcome = "applied";
  });
  await page.evaluate(() => {
    (
      window as unknown as { roleFixture: { currentHead: string } }
    ).roleFixture.currentHead = "different-head";
  });
  await dialog.getByRole("button", { name: "Check status" }).click();
  await expect(dialog).toBeVisible();
  await expect(
    dialog.getByRole("textbox", { name: "Role name" }).first(),
  ).toHaveValue("Code review");
  await page.evaluate(() => {
    (
      window as unknown as { roleFixture: { currentHead: string } }
    ).roleFixture.currentHead = "new-head";
  });
  await dialog.getByRole("button", { name: "Check status" }).click();
  await expect(dialog).toBeHidden();
});

test("conflict keeps the draft and latest review requires explicit replacement", async ({
  page,
}) => {
  const dialog = await openEditor(page);
  await dialog
    .getByRole("textbox", { name: "Role name" })
    .first()
    .fill("My unsaved role");
  await page.evaluate(() => {
    (
      window as unknown as { roleFixture: { saveResult: string } }
    ).roleFixture.saveResult = "conflict";
  });
  await dialog.getByRole("button", { name: "Save roles", exact: true }).click();
  await expect(dialog.getByText(/The canvas changed/)).toBeVisible();
  await expect(
    dialog.getByRole("textbox", { name: "Role name" }).first(),
  ).toHaveValue("My unsaved role");
  await dialog.getByRole("button", { name: "Load latest for review" }).click();
  await expect(
    dialog.getByText("Latest canvas — your draft has not changed"),
  ).toBeVisible();
  await expect(
    dialog.getByRole("textbox", { name: "Role name" }).first(),
  ).toHaveValue("My unsaved role");
  await dialog
    .getByRole("button", { name: "Discard my draft and use this version" })
    .click();
  await expect(
    dialog.getByRole("textbox", { name: "Role name" }).first(),
  ).toHaveValue("Review");
});

test("scope change fences save before command dispatch", async ({ page }) => {
  const dialog = await openEditor(page);
  await page.evaluate(() => {
    (window as unknown as { roleFixture: { generation: number } }).roleFixture
      .generation++;
  });
  await dialog.getByRole("button", { name: "Save roles", exact: true }).click();
  await expect(dialog.getByText(/Community or identity changed/)).toBeVisible();
  const count = await page.evaluate(
    () =>
      (window as unknown as { roleFixture: { saved: unknown[] } }).roleFixture
        .saved.length,
  );
  expect(count).toBe(0);
});

test("role deletion needs explicit holder resolution and adding a role focuses its name", async ({
  page,
}) => {
  const dialog = await openEditor(page);
  await dialog
    .getByRole("button", { name: "Remove role", exact: true })
    .first()
    .click();
  await expect(dialog.getByText(/Remove “Review” and clear/)).toBeVisible();
  await expect(dialog.getByRole("textbox", { name: "Role name" })).toHaveCount(
    2,
  );
  await dialog
    .getByRole("button", { name: "Remove role and references" })
    .click();
  await expect(dialog.getByRole("textbox", { name: "Role name" })).toHaveCount(
    1,
  );
  await dialog.getByRole("button", { name: "Add role", exact: true }).click();
  await expect(
    dialog.getByRole("textbox", { name: "Role name" }).last(),
  ).toBeFocused();
  await expect(
    dialog.getByRole("button", { name: "Save roles", exact: true }),
  ).toBeDisabled();
});

test("scope change during an in-flight save rejects its completion", async ({
  page,
}) => {
  const dialog = await openEditor(page);
  await dialog
    .getByRole("textbox", { name: "Role name" })
    .first()
    .fill("Keep my draft");
  await page.evaluate(() => {
    (
      window as unknown as { roleFixture: { deferSave: boolean } }
    ).roleFixture.deferSave = true;
  });
  await dialog.getByRole("button", { name: "Save roles", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as unknown as { roleFixture: { saved: unknown[] } })
            .roleFixture.saved.length,
      ),
    )
    .toBe(1);
  await page.evaluate(() => {
    const fixture = (
      window as unknown as {
        roleFixture: { generation: number; releaseSave?: () => void };
      }
    ).roleFixture;
    fixture.generation++;
    fixture.releaseSave?.();
  });
  await expect(dialog.getByText(/Community or identity changed/)).toBeVisible();
  await expect(
    dialog.getByRole("textbox", { name: "Role name" }).first(),
  ).toHaveValue("Keep my draft");
  await expect(dialog.getByText(/Canvas saved/)).toHaveCount(0);
});

test("a later live canvas read marks the confirmed save superseded", async ({
  page,
}) => {
  const dialog = await openEditor(page);
  await page.evaluate(() => {
    const fixture = (
      window as unknown as {
        roleFixture: { outcome: string; canvasHead: string };
      }
    ).roleFixture;
    fixture.outcome = "applied";
    fixture.canvasHead = "new-head";
  });
  await dialog.getByRole("button", { name: "Save roles", exact: true }).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByText(/Roles are current on the relay/)).toBeVisible();
  await page.evaluate(() => {
    const win = window as unknown as {
      roleFixture: { canvasHead: string };
      __BUZZ_E2E_EMIT_MOCK_MESSAGE__: (input: {
        channelName: string;
        kind: number;
        content: string;
        id: string;
        createdAt: number;
      }) => unknown;
    };
    win.roleFixture.canvasHead = "later-head";
    win.__BUZZ_E2E_EMIT_MOCK_MESSAGE__({
      channelName: "general",
      kind: 40100,
      content: "# New agreement",
      id: "c".repeat(64),
      createdAt: Math.floor(Date.now() / 1000) + 1,
    });
  });
  await expect(
    page.getByText("A newer canvas replaced your saved roles."),
  ).toBeVisible();
});

test("contact clear and an unassigned definition are included in one save", async ({
  page,
}) => {
  const dialog = await openEditor(page);
  await dialog
    .getByRole("combobox", { name: "Channel contact" })
    .selectOption("");
  await dialog.getByRole("button", { name: "Add role", exact: true }).click();
  await dialog
    .getByRole("textbox", { name: "Role name" })
    .last()
    .fill("Triage");
  await dialog
    .getByRole("textbox", { name: "Responsibilities and boundaries" })
    .last()
    .fill("Route new requests");
  await dialog.getByRole("button", { name: "Save roles", exact: true }).click();
  await expect(
    dialog.getByText(/Canvas saved; the working-agreement/),
  ).toBeVisible();
  const saved = await page.evaluate(
    () =>
      (
        window as unknown as {
          roleFixture: {
            saved: {
              draft: {
                contact: string | null;
                definitions: { label: string }[];
                assignments: Record<string, string>;
              };
            }[];
          };
        }
      ).roleFixture.saved,
  );
  expect(saved).toHaveLength(1);
  expect(saved[0].draft.contact).toBeNull();
  expect(saved[0].draft.definitions.map((entry) => entry.label)).toContain(
    "Triage",
  );
  expect(Object.values(saved[0].draft.assignments)).not.toContain("Triage");
});

test("failed save retains the form and Cancel or Escape discards only local edits", async ({
  page,
}) => {
  const dialog = await openEditor(page);
  await dialog
    .getByRole("textbox", { name: "Role name" })
    .first()
    .fill("Unsaved edit");
  await page.evaluate(() => {
    (
      window as unknown as { roleFixture: { saveError: boolean } }
    ).roleFixture.saveError = true;
  });
  await dialog.getByRole("button", { name: "Save roles", exact: true }).click();
  await expect(
    dialog.getByText(/Save unavailable before journal creation/),
  ).toBeVisible();
  await expect(
    dialog.getByRole("textbox", { name: "Role name" }).first(),
  ).toHaveValue("Unsaved edit");
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).toBeHidden();
  await page.getByTestId("channel-manage-roles").click();
  await expect(
    dialog.getByRole("textbox", { name: "Role name" }).first(),
  ).toHaveValue("Review");
  await dialog
    .getByRole("textbox", { name: "Role name" })
    .first()
    .fill("Keyboard cancel");
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  expect(
    await page.evaluate(
      () =>
        (window as unknown as { roleFixture: { saved: unknown[] } }).roleFixture
          .saved.length,
    ),
  ).toBe(1);
});

test("background canvas read failure preserves the open draft", async ({
  page,
}) => {
  const dialog = await openEditor(page);
  await dialog
    .getByRole("textbox", { name: "Role name" })
    .first()
    .fill("Keep through refresh");
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as unknown as { roleFixture: { canvasReads: number } })
            .roleFixture.canvasReads,
      ),
    )
    .toBeGreaterThanOrEqual(3);
  await page.evaluate(() => {
    const win = window as unknown as {
      roleFixture: { canvasReadError: boolean };
      __BUZZ_E2E_EMIT_MOCK_MESSAGE__: (input: {
        channelName: string;
        kind: number;
        content: string;
        createdAt: number;
      }) => unknown;
    };
    win.roleFixture.canvasReadError = true;
    win.__BUZZ_E2E_EMIT_MOCK_MESSAGE__({
      channelName: "general",
      kind: 40100,
      content: "# Changed",
      createdAt: Math.floor(Date.now() / 1000) + 1,
    });
  });
  await expect(
    page.getByText(
      "Canvas refresh failed. The previous version remains visible.",
    ),
  ).toBeVisible();
  await expect(
    dialog.getByRole("textbox", { name: "Role name" }).first(),
  ).toHaveValue("Keep through refresh");
  await expect(
    page.getByText("A newer canvas replaced your saved roles."),
  ).toHaveCount(0);
});
