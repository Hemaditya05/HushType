//! Transcript post-processing for HushType.
//!
//! Everything here is pure, synchronous and platform independent: the input is
//! the raw text produced by the speech recogniser and the output is the text
//! that gets inserted into the focused application. Rules are deliberately
//! conservative — the goal is to clean up dictation, never to rewrite it.

mod artifacts;
pub mod dictionary;
mod english;
mod numbers;
mod pipeline;
mod tokens;

pub use artifacts::{is_hallucination, strip_artifacts};
pub use dictionary::{default_terms, Dictionary, Term};
pub use pipeline::{process, AppContext, ProcessOptions};
