
#[macro_use]
#[doc(hide)]
pub extern crate log;
#[doc(hide)]
pub extern crate crossbeam;
#[doc(hide)]
pub extern crate glium;
#[doc(hide)]
pub extern crate image;
#[doc(hide)]
pub extern crate rand;
#[doc(hide)]
pub extern crate rayon;
#[doc(hide)]
pub extern crate vek;

/// Concurrent per-fragment painting.
pub mod frag;

/// Displaying pixels in an opengl window.
mod window;

// re-exports
pub use crossbeam::queue::SegQueue;

#[doc(transparent)]
pub use window::{
    open_window,
    Paint,
};

/// Re-exports of useful crates.
pub mod re {
    pub use crossbeam;
    pub use rand;
    pub use rayon;
    pub use image;
    pub use log;
    pub use vek;
}