//! Gameplay rules, kept free of any window or audio code so they can be tested on their own.
/// The engine version the game reports (Haxe: MainMenuState.psychEngineVersion). The update check compares the fork's gitVersion.txt with it.
pub const ENGINE_VERSION: &str = "1.0.4";

pub mod achievements;
pub mod chart;
pub mod clock;
pub mod content;
pub mod files;
pub mod haxe;
pub mod lang;
pub mod mod_settings;
pub mod play;
pub mod save;
pub mod title;
