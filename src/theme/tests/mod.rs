//! The theme engine's behavioural tests, split by the subject each one pins.
//!
//! They live in their own modules rather than one block at the foot of the engine
//! because they are the engine's largest single body of text, and a reader looking
//! for "what does a heading key promise?" should not have to walk the list-marker
//! cases to find out.

mod contrast;
mod headings;
mod lists;
mod merge;
mod sprites;
mod system;
