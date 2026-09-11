use super::BoundedFailure;
use super::{OwnedRecapRun, PrivateAskFailure, RecapStateFailure};

pub(super) fn finish_before_spawn(
    run: OwnedRecapRun,
    failure: PrivateAskFailure,
) -> PrivateAskFailure {
    match run.cleanup_before_spawn() {
        Ok(()) => failure,
        Err(state) => PrivateAskFailure::State(state),
    }
}

/// Preserve the durable pending marker when process ownership is uncertain.
pub(super) fn leave_process_pending(
    run: OwnedRecapRun,
    failure: BoundedFailure,
) -> PrivateAskFailure {
    drop(run);
    PrivateAskFailure::Process(failure)
}

pub(super) fn finish_after_process(
    run: OwnedRecapRun,
    failure: PrivateAskFailure,
) -> PrivateAskFailure {
    finish_after_process_with(
        run,
        failure,
        |run| run.mark_finished(),
        |run| run.cleanup_known_stopped(),
    )
}

/// The injected operations keep the error policy falsifiable without changing
/// the production OwnedRecapRun seam.
pub(super) fn finish_after_process_with(
    mut run: OwnedRecapRun,
    failure: PrivateAskFailure,
    mut mark_finished: impl FnMut(&mut OwnedRecapRun) -> Result<(), RecapStateFailure>,
    mut cleanup: impl FnMut(OwnedRecapRun) -> Result<(), RecapStateFailure>,
) -> PrivateAskFailure {
    if let Err(state) = mark_finished(&mut run) {
        return match cleanup(run) {
            Ok(()) => PrivateAskFailure::State(state),
            Err(cleanup) => PrivateAskFailure::State(cleanup),
        };
    }
    match cleanup(run) {
        Ok(()) => failure,
        Err(state) => PrivateAskFailure::State(state),
    }
}
