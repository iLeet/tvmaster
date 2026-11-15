//! Pure, deterministic domain logic for TVMaster: models, cursors and (later)
//! the scheduler and autopilot. No I/O lives here so it can be simulated with a
//! fake clock.

mod cursor;
mod model;

pub use cursor::EpisodeCursor;
pub use model::{Channel, RunOutPolicy, Strategy, Tv};
