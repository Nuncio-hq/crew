//! One admitted private Ask attempt: the owned run root, the fixed native
//! launch, and the developer entry point that reaches them.
//!
//! This module is the *execution* half of the private Ask. Its parent owns the
//! vocabulary — scope, request, selection, capability, admission — and every
//! safety decision still belongs there; nothing here relaxes a fence, and the
//! only reason it is a separate file is that the two halves together exceeded
//! the surface's size ceiling.

use super::{
    build_prompt, citations, credential, egress_proxy, finish_after_process, finish_before_spawn,
    leave_process_pending, leave_process_pending_state, profile, provider, runtime_paths,
    valid_profile, GroundedSource, PrivateAskAdmission, PrivateAskFailure, PrivateAskLaunchPlan,
    PRIVATE_ASK_INPUT_LIMIT, PRIVATE_ASK_OUTPUT_LIMIT, PRIVATE_ASK_STDERR_LIMIT,
    PRIVATE_ASK_TIMEOUT,
};
use crate::managed_agents::discovery::bounded_command::{
    output_with_policy_and_spawn_hook, BoundedFailure, BoundedPolicy, OutputBudget,
};
use crate::managed_agents::recap_capability::{same_executable_proof, RecapExecutableIdentity};
use crate::managed_agents::recap_ownership::VerifiedStagingOwnership;
use crate::managed_agents::recap_state::OwnedRecapRun;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Run one private Ask from the developer surface.
///
/// This is the production entry point the dev command calls: every safety
/// decision belongs to [`admit_private_ask`] and [`PrivateAskAttempt::run`],
/// and this function's job is to reach them with a real selection or to refuse
/// with the typed reason it could not.
///
/// It refuses at the first fence it cannot pass. Today that fence is the
/// binding itself: nothing in the desktop yet resolves the developer surface's
/// question to a selected managed agent, an immutable Wiki snapshot and the
/// retained capability probe for that agent's exact binary. Until a producer
/// exists for all three there is no selection to admit, and `AgentUnbound` is
/// the honest answer — not a placeholder result, and not a refusal about a
/// dimension that is now provable.
///
/// The egress bound is no longer the blocker: `PrivateAskCapability::from_probe`
/// projects it from the attempt proxy's own record.
pub(crate) fn dev_run(question: &str) -> Result<PrivateAskResponse, PrivateAskFailure> {
    if question.trim().is_empty() || question.contains('\0') {
        return Err(PrivateAskFailure::InvalidQuestion);
    }
    if question.len() > PRIVATE_ASK_INPUT_LIMIT {
        return Err(PrivateAskFailure::InputLimit);
    }
    Err(PrivateAskFailure::AgentUnbound)
}

/// Result from a completed owned attempt; citations are authenticated request grounding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrivateAskResponse {
    pub attempt_id: String,
    pub session_generation: String,
    pub markdown: String,
    pub citations: Vec<GroundedSource>,
    /// What this run's own egress proxy observed. It travels with the answer
    /// because it is the evidence a capability decision is made from: a probe
    /// capture reads it here rather than reconstructing what it thinks happened.
    pub egress: egress_proxy::EgressObservation,
}

pub(crate) struct PrivateAskAttempt {
    // Visible to the parent module and its test modules, which is where the
    // attempt's fences are exercised; nothing outside `private_ask` can name
    // them, so the run root and cancel flag stay owned by this type.
    pub(super) admission: PrivateAskAdmission,
    pub(super) ownership: VerifiedStagingOwnership,
    pub(super) run: Option<OwnedRecapRun>,
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) attempt_id: String,
    pub(super) profile_staged: bool,
}

impl PrivateAskAttempt {
    /// Create a fresh owned run below the verified managed-agent base.
    pub(crate) fn create(
        admission: PrivateAskAdmission,
        ownership: VerifiedStagingOwnership,
        now: u64,
    ) -> Result<Self, PrivateAskFailure> {
        let base = ownership.recap_base().map_err(PrivateAskFailure::State)?;
        let run = OwnedRecapRun::create(&base, now).map_err(PrivateAskFailure::State)?;
        Ok(Self {
            admission,
            ownership,
            run: Some(run),
            cancel: Arc::new(AtomicBool::new(false)),
            attempt_id: uuid::Uuid::new_v4().to_string(),
            profile_staged: false,
        })
    }

    /// Signal only this attempt.  The bounded runner owns process teardown.
    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    /// Stable attempt identifier for stream correlation and retries.
    pub(crate) fn attempt_id(&self) -> &str {
        &self.attempt_id
    }

    /// Copy one approved staging profile into this attempt's private Hermes home.
    pub(crate) fn stage_hermes_profile(&mut self, source: &Path) -> Result<(), PrivateAskFailure> {
        if self.admission.state.runtime_id != "hermes" {
            return Err(PrivateAskFailure::MissingProfile);
        }
        let approved_root = self
            .ownership
            .hermes_profile_source()
            .map_err(PrivateAskFailure::State)?;
        let profile = self
            .admission
            .state
            .profile
            .as_deref()
            .ok_or(PrivateAskFailure::MissingProfile)?;
        if profile == crate::managed_agents::hermes_profile::HERMES_HOME_PROFILE_NAME
            || !valid_profile(profile)
        {
            return Err(PrivateAskFailure::ProfileUnavailable);
        }
        // The native receipt grants exactly one profile directory.  Compare
        // the caller's path lexically before canonicalization so aliases such
        // as `../` or a symlink cannot broaden that grant.
        let expected = approved_root.join(profile);
        if source != expected {
            return Err(PrivateAskFailure::ProfileUnavailable);
        }
        let source = profile::canonical_profile_source(source)?;
        if source != expected {
            return Err(PrivateAskFailure::ProfileUnavailable);
        }
        let run = self.run.as_ref().ok_or(PrivateAskFailure::InvalidState)?;
        let hermes_root = run.path().join("hermes");
        profile::ensure_private_profile_directory(&hermes_root)?;
        let profiles = hermes_root.join("profiles");
        profile::ensure_private_profile_directory(&profiles)?;
        let destination = profiles.join(profile);
        profile::copy_private_profile(&source, &destination)?;
        self.profile_staged = true;
        Ok(())
    }

    /// The single destination this attempt's egress proxy may reach.
    ///
    /// For hermes it comes from the profile copy already staged inside the run
    /// root — the bytes the child will actually read — so the bound destination
    /// and the configured one cannot disagree.
    fn provider_host(
        &self,
        run: &OwnedRecapRun,
    ) -> Result<egress_proxy::ProviderHost, PrivateAskFailure> {
        let configured = if self.admission.state.runtime_id == "hermes" {
            let profile = self
                .admission
                .state
                .profile
                .as_deref()
                .ok_or(PrivateAskFailure::MissingProfile)?;
            provider::staged_hermes_provider(
                &run.path().join("hermes").join("profiles").join(profile),
            )
        } else {
            None
        };
        provider::provider_host(&self.admission.state.runtime_id, configured.as_deref())
    }

    /// The provider credential this attempt hands to its child, if any.
    ///
    /// Under test the platform secret store is not the thing being examined —
    /// and must not be touched by a unit test — so a known canary replaces the
    /// keychain read. The gate it is subject to is NOT replaced: the canary is
    /// still refused unless the proxy is serving, and `credential_tests` drives
    /// the real `stage_private_ask_credential` against that same gate. The
    /// canary exists so the privacy tests can prove where a credential does and
    /// does not end up on the real launch path.
    #[cfg(test)]
    pub(crate) const TEST_CREDENTIAL_CANARY: &'static str =
        "canary-private-ask-credential-must-not-leak";

    #[cfg(test)]
    fn stage_credential(
        &self,
        _state_dir: &Path,
        proxy: &egress_proxy::EgressProxy,
    ) -> Result<Option<credential::StagedCredential>, PrivateAskFailure> {
        if !proxy.is_listening() {
            return Err(PrivateAskFailure::EgressBoundUnverified);
        }
        match self.admission.state.runtime_id.as_str() {
            "claude" => credential::claude_credential(Self::TEST_CREDENTIAL_CANARY).map(Some),
            "hermes" => Ok(None),
            _ => Err(PrivateAskFailure::MissingRuntime),
        }
    }

    #[cfg(not(test))]
    fn stage_credential(
        &self,
        state_dir: &Path,
        proxy: &egress_proxy::EgressProxy,
    ) -> Result<Option<credential::StagedCredential>, PrivateAskFailure> {
        credential::stage_private_ask_credential(
            &self.admission.state.runtime_id,
            state_dir,
            proxy,
            &self.cancel,
        )
    }

    /// Execute one fixed native plan and clean only its finished generation.
    pub(crate) fn run(mut self) -> Result<PrivateAskResponse, PrivateAskFailure> {
        let mut run = self.run.take().ok_or(PrivateAskFailure::InvalidState)?;
        if let Err(failure) = self
            .ownership
            .recap_base()
            .map_err(PrivateAskFailure::State)
        {
            return Err(finish_before_spawn(run, failure));
        }
        if self.admission.state.runtime_id == "hermes" && !self.profile_staged {
            return Err(finish_before_spawn(run, PrivateAskFailure::MissingProfile));
        }
        // Egress is bounded before anything else is prepared: the proxy's port
        // goes into the Seatbelt policy and into the child's environment, so it
        // must be settled before the plan exists. The handle lives for the rest
        // of this function, and every return path below drops it, which stops
        // the listener — there is no exit that leaves the port open.
        let provider_host = match self.provider_host(&run) {
            Ok(host) => host,
            Err(failure) => return Err(finish_before_spawn(run, failure)),
        };
        let proxy = match egress_proxy::EgressProxy::start(provider_host) {
            Ok(proxy) => proxy,
            Err(failure) => return Err(finish_before_spawn(run, failure)),
        };
        let plan = match PrivateAskLaunchPlan::for_admission(&self.admission, &run, &proxy) {
            Ok(plan) => plan,
            Err(failure) => return Err(finish_before_spawn(run, failure)),
        };
        // Read only after the proxy is serving: `stage_private_ask_credential`
        // refuses otherwise, so a bearer token cannot exist for a child whose
        // egress is not already bounded.
        let credential = match self.stage_credential(run.path(), &proxy) {
            Ok(credential) => credential,
            Err(failure) => return Err(finish_before_spawn(run, failure)),
        };
        let prompt = match build_prompt(&self.admission.request, &self.admission.state.persona) {
            Ok(prompt) => prompt,
            Err(failure) => return Err(finish_before_spawn(run, failure)),
        };
        // A withdrawn request does no further work. Checking here keeps
        // cancellation the reported outcome rather than whatever the next
        // fence happens to notice about a run nobody still wants.
        if self.cancel.load(Ordering::SeqCst) {
            return Err(finish_before_spawn(
                run,
                PrivateAskFailure::Process(BoundedFailure::Cancelled),
            ));
        }
        // Re-hash the executable immediately before building the command.
        // The capability was certified against specific bytes, and an upgrade
        // or a shim swapped in between admission and launch would otherwise run
        // under a proof that no longer describes it.
        if let Err(failure) = same_executable_now(&self.admission.capability.executable) {
            return Err(finish_before_spawn(run, failure));
        }
        // A hard link inside the run root to an outside inode would turn the
        // policy's run-root write allowance into a write allowance for that
        // outside file: Seatbelt matches the path, and both names reach the
        // same bytes. Checked immediately before spawn, after profile staging.
        if let Err(failure) = runtime_paths::assert_no_hard_links(run.path()) {
            return Err(finish_before_spawn(run, failure));
        }
        let mut command = plan.command();
        if let Some(credential) = credential.as_ref() {
            // Environment only. Never argv, never a file in the run root.
            let (name, value) = credential.env_entry();
            command.env(name, value);
        }
        if plan.prompt_on_stdin {
            let input = match run.input(prompt.as_bytes()) {
                Ok(input) => input,
                Err(failure) => {
                    return Err(finish_before_spawn(run, PrivateAskFailure::State(failure)));
                }
            };
            command.stdin(Stdio::from(input));
        } else {
            command.stdin(Stdio::null());
            command.arg(prompt);
        }
        if let Err(failure) = run.mark_process_pending() {
            return Err(finish_before_spawn(run, PrivateAskFailure::State(failure)));
        }
        // Persist the owned child PID before any output is consumed. Without it
        // a crash between spawn and completion leaves recovery with no process
        // identity, so the root stays pending forever instead of being reaped.
        let mut pid_error = None;
        let output = output_with_policy_and_spawn_hook(
            command,
            BoundedPolicy {
                timeout: PRIVATE_ASK_TIMEOUT,
                budget: OutputBudget::PerStream {
                    stdout: PRIVATE_ASK_OUTPUT_LIMIT,
                    stderr: PRIVATE_ASK_STDERR_LIMIT,
                },
            },
            &self.cancel,
            |pid| {
                run.mark_process_started(pid).map_err(|failure| {
                    pid_error = Some(failure);
                    BoundedFailure::Cleanup
                })
            },
        );
        let output = match output {
            Ok(outcome) => outcome.output,
            Err(BoundedFailure::Cleanup) => {
                // Keep the durable pending marker when teardown is uncertain, or
                // when the owned PID could not be recorded: in both cases the
                // process boundary is unknown and recovery must retain the root.
                if let Some(failure) = pid_error {
                    return Err(leave_process_pending_state(run, failure));
                }
                return Err(leave_process_pending(run, BoundedFailure::Cleanup));
            }
            Err(failure) => {
                let failure = PrivateAskFailure::Process(failure);
                return Err(finish_after_process(run, failure));
            }
        };
        if let Err(failure) = run.mark_finished() {
            return Err(finish_after_process(run, PrivateAskFailure::State(failure)));
        }
        let markdown =
            match plan.parse_output(output.status.success(), &output.stdout, &output.stderr) {
                Ok(markdown) => markdown,
                Err(failure) => return Err(finish_after_process(run, failure)),
            };
        // Stop the proxy and take its record before the run root is cleaned:
        // after this point nothing may reach the network on this attempt's
        // behalf, and the record is final. A production run has no off-provider
        // listener of its own, so the direct-connection count is zero here by
        // construction; a probe capture supplies its own.
        let egress = proxy.observe(0);
        // The answer's own citations, resolved against the verified snapshot.
        // Copying the request's grounding here would make `citations` a
        // restatement of the input and say nothing about what the runtime
        // actually used — or reached.
        let citations = match citations::resolve(&markdown, &self.admission.request.grounding) {
            Ok(citations) => citations,
            Err(failure) => {
                // The run root is still closed out: a refused answer is a
                // finished attempt, not an abandoned one.
                run.cleanup().map_err(PrivateAskFailure::State)?;
                return Err(failure);
            }
        };
        let response = PrivateAskResponse {
            attempt_id: self.attempt_id,
            session_generation: self.admission.state.session_generation.clone(),
            markdown,
            citations,
            egress,
        };
        run.cleanup().map_err(PrivateAskFailure::State)?;
        Ok(response)
    }
}

/// Confirm the certified executable is still the file that will be run.
///
/// The two refusals are kept apart because they mean different things to the
/// viewer: a runtime that has gone missing is an installation problem, while a
/// runtime whose bytes changed under its own proof is a changed selection and
/// must be re-certified.
fn same_executable_now(certified: &RecapExecutableIdentity) -> Result<(), PrivateAskFailure> {
    use crate::managed_agents::recap_capability::RecapFailure;
    let observed = crate::managed_agents::recap_capability::verify_executable(certified).map_err(
        |failure| match failure {
            RecapFailure::MissingExecutable => PrivateAskFailure::MissingRuntime,
            _ => PrivateAskFailure::SelectionChanged,
        },
    )?;
    same_executable_proof(&observed, certified)
        .then_some(())
        .ok_or(PrivateAskFailure::SelectionChanged)
}
