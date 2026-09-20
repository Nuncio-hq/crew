//! Native picker grants and authenticated Wiki Source access.
mod access;
mod admission;
mod grants;
mod operations;

pub(crate) use operations::{
    wiki_choose_source_root, wiki_forget_source_root, wiki_open_verified_source,
    wiki_source_grants, SourceState,
};

/// One native source grant, exposed to the rest of the app as a read capability
/// and nothing more.
///
/// The private Ask resolver needs to read source bytes the viewer already chose
/// a folder for, but it must not be able to name a folder, install a grant, or
/// see the anchor's internals. This handle carries only the read.
pub(crate) struct NativeSourceRoot {
    grant: std::sync::Arc<grants::Grant>,
}

impl NativeSourceRoot {
    /// The grant currently installed for this repository coordinate, if the
    /// viewer chose one in this exact owner/workspace scope.
    ///
    /// `None` is a legitimate answer — a caller that cannot read source bytes
    /// simply has no grounding — so this returns an option rather than an
    /// error the caller would have to interpret.
    pub(crate) fn current(
        state: &SourceState,
        token: &crate::app_state::owner_scope::OwnerScopeToken,
        coordinate: &str,
    ) -> Option<Self> {
        state
            .grant_for_coordinate(token, coordinate)
            .map(|grant| Self { grant })
    }

    /// The workspace mode (`git` / `folder`) the grant was anchored to. A
    /// snapshot whose source revision does not start with it describes a
    /// different checkout of the same repository.
    pub(crate) fn workspace_mode(&self) -> &str {
        &self.grant.anchor.workspace_mode
    }

    /// Read one authenticated source reference through the granted root.
    pub(crate) fn read_verified_reference(
        &self,
        revision: &str,
        reference: &crew_wiki::source_snapshot::SourceReference,
        deadline: std::time::Instant,
    ) -> Result<crew_wiki::source_access::VerifiedSourceFile, String> {
        self.grant
            .root
            .read_verified_reference(revision, reference, deadline)
            .map_err(|error| error.to_string())
    }
}
