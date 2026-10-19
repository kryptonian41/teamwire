//! Reads the "Decisions I made" blocks of members' reports, for one team,
//! for the teams with a ticket ref, or for every block that matches a text.
//! Nothing is written.

mod find;

pub use find::{Block, find_blocks};
