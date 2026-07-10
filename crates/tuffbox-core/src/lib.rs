//! TuffBox core.
//!
//! This crate contains deterministic project, graph and resolver logic.
//! AI must not be used inside this crate.

pub mod change_plan;
pub mod crash;
pub mod diagnostics;
pub mod exporter;
pub mod graph;
pub mod http;
pub mod importer;
pub mod launcher;
pub mod lockfile;
pub mod manifest;
pub mod provider;
pub mod resolver;
pub mod snapshot;
pub mod time_util;
pub mod versions;
pub mod mc_install;
pub mod forge_install;
pub mod jre;
pub mod process;
pub mod mod_files;

pub mod environment;
pub mod unified;
pub mod adapters;
pub mod tag_normalizer;
pub mod overrides;
pub mod registry;
pub mod knowledge;
pub mod properties_parser;
pub mod level_dat;
pub mod crash_assistant;
pub mod ai_explanation;
pub mod recipe_layout;
pub mod recipe_scan;

pub use change_plan::*;
pub use crash::*;
pub use diagnostics::*;
pub use exporter::*;
pub use graph::*;
pub use importer::*;
pub use launcher::*;
pub use lockfile::*;
pub use manifest::*;
pub use provider::*;
pub use resolver::*;
pub use snapshot::*;
pub use mod_files::*;
pub use versions::*;
pub use mc_install::*;
pub use jre::*;
pub use process::*;
pub use http::*;
pub use forge_install::*;

