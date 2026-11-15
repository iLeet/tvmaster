//! Pure, deterministic domain logic for TVMaster: models, cursors, timelines
//! and (later) the scheduler and autopilot. No I/O lives here so it can be
//! simulated with a fake clock.

mod cursor;
mod model;
mod ordering;
mod timefmt;
mod timeline;

pub use cursor::EpisodeCursor;
pub use model::{Channel, RunOutPolicy, Strategy, Tv};
pub use ordering::{order_sequential, order_unsorted, MediaRef};
pub use timefmt::xmltv_time;
pub use timeline::{Entry, Slot, SlotIter, Timeline};
