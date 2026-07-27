//! Composable view components.
//!
//! Each component owns its own state (if it has any) and knows how to draw
//! itself into a rect. The application composes them; components never reach
//! back into the application.

pub mod category;
pub mod detail;
pub mod header;
pub mod help;
pub mod scanning;
pub mod search;
pub mod sourcecards;
pub mod statusbar;
pub mod table;
