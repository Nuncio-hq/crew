import assert from "node:assert/strict";
import { test } from "node:test";

import {
  PERSONA_CREATE_RUNTIME_UNAVAILABLE_MESSAGE,
  personaSubmitFeedbackSurface,
  shouldShowStandaloneRuntimeUnavailableWarning,
} from "./personaSubmitFeedback.ts";

test("personaSubmitFeedbackSurface_catalogWhenCatalogOpen", () => {
  assert.equal(personaSubmitFeedbackSurface(true), "catalog");
});

test("personaSubmitFeedbackSurface_libraryWhenCatalogClosed", () => {
  assert.equal(personaSubmitFeedbackSurface(false), "library");
});

test("personaCreateRuntimeUnavailableMessage_namesHarnessInstall", () => {
  assert.match(PERSONA_CREATE_RUNTIME_UNAVAILABLE_MESSAGE, /harness/i);
  assert.match(PERSONA_CREATE_RUNTIME_UNAVAILABLE_MESSAGE, /Settings/i);
});

test("standaloneRuntimeWarning_defaultsCreateWithWarning", () => {
  assert.equal(
    shouldShowStandaloneRuntimeUnavailableWarning({
      isCreateMode: true,
      hasRuntimeWarning: true,
      aiConfigurationMode: "defaults",
    }),
    true,
  );
});

test("standaloneRuntimeWarning_hiddenInCustomizeHarnessShowsIt", () => {
  assert.equal(
    shouldShowStandaloneRuntimeUnavailableWarning({
      isCreateMode: true,
      hasRuntimeWarning: true,
      aiConfigurationMode: "custom",
    }),
    false,
  );
});

test("standaloneRuntimeWarning_editModeNever", () => {
  assert.equal(
    shouldShowStandaloneRuntimeUnavailableWarning({
      isCreateMode: false,
      hasRuntimeWarning: true,
      aiConfigurationMode: "defaults",
    }),
    false,
  );
});
