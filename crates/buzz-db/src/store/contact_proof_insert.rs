//! Test-only re-export shim: the contact classes were promoted to the
//! production `store::contact` module by issue #412. Existing R4 fixture
//! tests keep importing through this path unchanged.

pub(crate) use crate::contact::ContactClass;
