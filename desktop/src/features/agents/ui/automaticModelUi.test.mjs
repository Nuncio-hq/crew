import assert from "node:assert/strict";
import test from "node:test";
import {
  decorateAutomaticModelOptions,
  modelAfterAutomaticDropdownChange,
  resolveAutomaticModelUiState,
} from "./automaticModelUi.ts";
import { AUTO_MODEL_DROPDOWN_VALUE } from "./agentConfigOptions.tsx";

const inherited = {
  value: AUTO_MODEL_DROPDOWN_VALUE,
  label: "Default model (configured-model)",
};
const explicit = { value: "explicit-model", label: "Explicit model" };

test("environment-backed models retain inherited defaults and their provenance label", () => {
  assert.deepEqual(
    decorateAutomaticModelOptions([inherited, explicit], {
      allowInheritedModel: true,
      isRelayMesh: false,
      offerAutomaticModel: false,
      selectableAutoModel: false,
    }),
    [inherited, explicit],
  );
});

test("native runtimes without automatic selection omit the empty model option", () => {
  assert.deepEqual(
    decorateAutomaticModelOptions([inherited, explicit], {
      isRelayMesh: false,
      offerAutomaticModel: false,
      selectableAutoModel: false,
    }),
    [explicit],
  );
});

test("Cursor keeps its explicit Auto router label", () => {
  assert.equal(
    decorateAutomaticModelOptions([inherited], {
      isRelayMesh: false,
      offerAutomaticModel: true,
      selectableAutoModel: true,
    })[0].label,
    "Auto",
  );
});

test("runtime-backed inherited selection remains blank when submitted", () => {
  const state = resolveAutomaticModelUiState({
    isRelayMesh: false,
    model: "",
    modelSelectValue: AUTO_MODEL_DROPDOWN_VALUE,
    runtime: { id: "buzz-agent", modelEnvVar: "BUZZ_AGENT_MODEL" },
  });
  assert.deepEqual(
    decorateAutomaticModelOptions([inherited, explicit], {
      isRelayMesh: false,
      ...state,
    }),
    [inherited, explicit],
  );
  assert.equal(
    modelAfterAutomaticDropdownChange({
      isRelayMesh: false,
      nextValue: AUTO_MODEL_DROPDOWN_VALUE,
      nextSelectionModel: "",
      selectableAutoModel: state.selectableAutoModel,
    }),
    "",
  );
});
