import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const srcRoot = join(here, "../..");

function readSrc(relativePath) {
  return readFileSync(join(srcRoot, relativePath), "utf8");
}

test("office chrome tokens name header, field, and composer surfaces", async () => {
  const chrome = await import("./officeChrome.ts");
  assert.match(
    chrome.OFFICE_HEADER_BAR_CLASS,
    /backdrop-blur|bg-background/,
    "header bar must reuse office glass / bar chrome",
  );
  assert.match(
    chrome.OFFICE_FIELD_BOX_CLASS,
    /border-input/,
    "field box must use a readable input border, not a wash",
  );
  assert.doesNotMatch(
    chrome.OFFICE_FIELD_BOX_CLASS,
    /border-input\/40/,
    "field box must not use the faint /40 border that blends into dark chrome",
  );
  assert.match(
    chrome.OFFICE_COMPOSER_SURFACE_CLASS,
    /rounded-2xl/,
    "ask/composer must be a distinct rounded surface",
  );
  assert.equal(chrome.OFFICE_SURFACE.headerBar, "header-bar");
  assert.equal(chrome.OFFICE_SURFACE.fieldBox, "field-box");
  assert.equal(chrome.OFFICE_SURFACE.composerSurface, "composer-surface");
});

test("wiki home is search + repo list, not a CMS create form", () => {
  const library = readSrc("features/wiki/ui/WikiLibraryScreen.tsx");
  assert.doesNotMatch(
    library,
    /Create company page/,
    "Wiki home must not host a Create company page CMS form",
  );
  assert.doesNotMatch(
    library,
    /WikiCompanyEditor/,
    "Wiki home must not mount the company-page authoring form",
  );
  assert.match(
    library,
    /wiki-home-search/,
    "Wiki home must expose a search field",
  );
  assert.match(
    library,
    /Which repo would you like to understand/,
    "Wiki home heading must copy the DeepWiki index IA",
  );
  assert.match(library, /WikiRepoCard/, "Wiki home must list repo wikis");
});

test("org roster editor labels sit above field boxes, not on inputs", () => {
  const roster = readSrc("features/org/ui/OrgRosterEditor.tsx");
  const field = readSrc("shared/ui/OfficeField.tsx");
  assert.match(
    roster,
    /OfficeField/,
    "roster must use a labeled field that keeps the label off the input",
  );
  assert.match(
    field,
    /OFFICE_SURFACE\.fieldBox|data-office-surface=["']field-box["']/,
    "OfficeField must mark the office field-box surface",
  );
  assert.doesNotMatch(
    roster,
    /<label[\s\S]{0,200}<(select|Input|textarea)/,
    "labels must not wrap the control (that is the fail fixture: label sits on input)",
  );
});

test("wiki page header is a bar and ask is a composer surface", () => {
  const page = readSrc("features/wiki/ui/WikiPageView.tsx");
  const ask = readSrc("features/wiki/ui/WikiAskBox.tsx");
  const header = readSrc("features/wiki/ui/WikiHeaderControls.tsx");
  assert.match(
    page,
    /OFFICE_SURFACE\.headerBar|data-office-surface=["']header-bar["']/,
    "repo wiki header must read as an office header bar",
  );
  assert.match(
    header,
    /wiki-generate-mirror/,
    "Generate stays a header action",
  );
  assert.match(
    ask,
    /OFFICE_SURFACE\.composerSurface|data-office-surface=["']composer-surface["']/,
    "ask bar must be a distinct composer surface",
  );
});
