# Profile lifecycle hardening verification

Boundary: Crew-owned Hermes readiness projection, archive lifecycle, and
profile offboarding UI from issue #119. Semantics reference D-035 and spike
0015.

## Automated evidence

The desktop Rust suite passed with 2398 tests, zero failures, and 14 ignored.
The frontend check, typecheck, full test suite, file-size, and diff checks
passed in the implementation handoff. The frontend suite reported 5050 passed,
zero failed, and one skipped.

## Readiness and lifecycle

The Rust contract tests use temporary `HERMES_HOME` directories, fake Hermes
commands, and injected archive roots. They cover missing directories, invalid
config, archive round trips, exclusions, traversal rejection, running-agent
refusal, and successful cleanup after the guard clears.

## Headless evidence status

No Hermes binary is installed on this machine. A real Hermes auth success probe
is therefore not verifiable here; a machine with Hermes installed and a truthful
headless authentication probe must run the readiness auth contract. The local
filesystem and archive behavior remains exercised through the real Rust code.

## Playwright and screenshots

The requested headless Playwright/screenshot run was not completed in this
environment. See `/home/ubuntu/119-evidence/playwright-run.txt`. No screenshot
hash table is claimed; this avoids presenting simulated or stale UI captures as
verification.

## Artifacts

- `/home/ubuntu/119-evidence/playwright-run.txt`
- `/home/ubuntu/119-evidence/readiness-fault-injection.txt`
- `/home/ubuntu/119-evidence/README.txt`
