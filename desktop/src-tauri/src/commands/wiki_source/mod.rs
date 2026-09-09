//! Native picker grants and authenticated Wiki Source access.
mod access;
mod admission;
mod grants;
mod operations;

pub(crate) use operations::{
    wiki_choose_source_root, wiki_forget_source_root, wiki_open_verified_source,
    wiki_source_grants, SourceState,
};
