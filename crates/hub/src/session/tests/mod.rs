//! The live session's tests: how its status follows what's recorded, what it takes in from its agent's worker, the
//! claims its runs are held to, what it adds to a turn the agent started by itself, the failures it says repeat, and the
//! lines said for its agent while it can't answer.

mod capabilities;
mod covering;
mod failing;
mod ingest;
mod own_turns;
mod status;
