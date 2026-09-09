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
