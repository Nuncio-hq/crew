/// Result category for a parameterized-replaceable event write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterizedReplaceStatus {
    /// The incoming event was inserted as the coordinate's live head.
    Inserted,
    /// The exact event was already accepted.
    Duplicate,
    /// The event ID exists, but is not the coordinate's current live head.
    DuplicateNotLive,
    /// A newer event, or lower-ID same-second event, already dominates it.
    Superseded,
    /// A requested current revision has no live coordinate head.
    RevisionMissing,
    /// The live coordinate head differs from the requested revision.
    RevisionMismatch,
    /// An exact replay was required, but the event is not the live head.
    ReplayOnlyMiss,
    /// Conditional v1 Wiki `_toc` only: the exact submitted head was accepted
    /// at this same community/owner/kind/`d` coordinate and is now soft
    /// deleted. Its exact event identity can never become live again through
    /// ordinary replacement, so the submitted head is permanently retired.
    WikiHeadRetired,
    /// Conditional v1 Wiki `_toc` only: the coordinate has no live head, the
    /// precondition names an exact expected revision, and that expected event
    /// was accepted at this same coordinate and is now soft deleted. The
    /// precondition can therefore never be satisfied again.
    WikiExpectedHeadRetired,
}

/// Structural precondition for a parameterized-replaceable write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterizedReplacePrecondition<'a> {
    /// Apply normal NIP-33 ordering without a revision precondition.
    Unconditional,
    /// Require no live head, while still accepting exact current replay.
    ExpectedMissing,
    /// Reject replacement when the locked live head carries this tag/value.
    RejectIfLiveHeadHasTag(&'a str, &'a str),
    /// Require the live head to match this validated event ID.
    ExpectedRevision(&'a [u8]),
    /// Accept only an exact live-head replay and perform no mutation otherwise.
    ExactReplayOnly,
}
