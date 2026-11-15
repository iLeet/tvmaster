//! Pure, deterministic domain logic for TVMaster: models, cursors, lineups and
//! the scheduler. No I/O lives here so it can be simulated with a fake clock.

mod catalog;
mod clock;
mod cursor;
mod lineup;
mod model;
mod scheduler;
mod timefmt;

pub use catalog::{Catalog, Ep, MediaRef, ShowEps};
pub use clock::Zone;
pub use cursor::EpisodeCursor;
pub use lineup::{Advance, Block, CatchUp, Event, Lineup, Order, Season, Source};
pub use model::{Channel, RunOutPolicy, Strategy, Tv};
pub use scheduler::{
    align_start, commit_aired, generate, rebuild_reservations, Cursors, ItemKind, SchedConfig,
    ScheduledItem,
};
pub use timefmt::xmltv_time;
