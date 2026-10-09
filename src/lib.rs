//! ArtCraft Launcher — a library, so the icon generator and the headless CLI
//! share the same code as the GUI.

pub mod anim;
pub mod apps;
pub mod config;
pub mod gfx;
pub mod github;
pub mod hero;
pub mod installer;
pub mod logo;
pub mod osint;
pub mod paths;
pub mod render;
pub mod screens;
pub mod state;
pub mod telemetry;
pub mod theme;
pub mod ui;
pub mod window;

pub use theme::{AppAccent, Tokens};
