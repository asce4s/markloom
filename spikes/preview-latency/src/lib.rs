//! Throwaway Typst compile/render path for the Milestone 0 latency spike.
//! The same functions back the headless `measure` binary and the GPUI window.

mod world;

pub use world::{
    PageImage, Report, SpikeWorld, Timing, compile_and_render, render_page, twenty_page_body,
    wrap_body,
};
