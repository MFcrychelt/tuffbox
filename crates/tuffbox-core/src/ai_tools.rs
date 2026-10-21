//! Tool specifications for the AI agent loop.
//!
//! The catalogue lives in the core crate because it is *data*: names,
//! descriptions and JSON Schemas. What each tool actually does is the
//! dispatcher's job (the desktop crate maps a tool name onto an existing
//! command), so nothing here depends on Tauri.
//!
//! One tool per *capability*, not per upstream tool name: three readers and
//! three writers of the same config file become one read and one write. Fewer
//! schemas means fewer chances for the model to pick the wrong one, and one
//! place to keep the description honest when the underlying command changes.
//!
//! Everything declared here is **read-only** by design. Anything that mutates
//! the project goes through an `ActionPlan` the user confirms — a tool must
//! never be a second, unconfirmed path to the same change.

use serde::Serialize;
use serde_json::{json, Value};

/// One tool the agent may call. `parameters` is a JSON Schema object.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema (`{"type":"object","properties":{…},"required":[…]}`).
    pub parameters: Value,
}

impl ToolSpec {
    fn new(name: &str, description: &str, parameters: Value) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            parameters,
        }
    }

    /// OpenAI `tools[]` entry (`{"type":"function","function":{…}}`).
    pub fn to_openai(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

/// No arguments — the tool takes the current project from the caller's context.
fn no_args() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

/// System prompt for the tool-using agent.
///
/// The tool catalogue is read-only, so the prompt's job is to keep the model
/// inside that boundary: inspect freely, propose (never claim) changes.
pub const AGENT_SYSTEM_PROMPT: &str = r#"You are TuffBox, the assistant inside a Minecraft modpack IDE.

You may call the tools you were given to inspect the open pack: its mods, worlds, configs, logs, crashes, Java runtimes and updates. Prefer looking things up over guessing — a tool call is cheaper than a wrong answer.

Hard rules:
1. Never claim you changed, installed, removed or edited anything. You cannot: the tools are read-only. If a change is needed, describe it and say the user must apply it (the launcher shows a confirmable plan for that).
2. Report what the tools returned, including errors. If a tool failed, say which one and what it said, then answer with what you do know.
3. Keep the final answer short and concrete: what you found, what it means, what to do next. No raw dumps — summarise the tool output.
4. If the question is about something the tools cannot show, say so instead of inventing details."#;

/// The read-only catalogue.
///
/// Every entry corresponds to a capability the launcher already has; the
/// dispatcher is expected to fail loudly (`unknown tool`) rather than silently
/// no-op when one of them has no implementation yet.
pub fn tool_catalogue() -> Vec<ToolSpec> {
    vec![
        ToolSpec::new(
            "pack_health",
            "Aggregate health of the current pack: diagnostics, dependency graph problems, \
             duplicate and wrong-loader mods, quest validation, newest crash.",
            no_args(),
        ),
        ToolSpec::new(
            "diagnose_build",
            "Analyse the latest run of the current pack: crash report and log tail, with the \
             detected cause and suggested fixes.",
            json!({
                "type": "object",
                "properties": {
                    "report_id": {
                        "type": "string",
                        "description": "Specific crash report id; omit for the newest one."
                    }
                },
                "additionalProperties": false
            }),
        ),
        ToolSpec::new(
            "read_log_tail",
            "Return the last lines of the game log for the current pack.",
            json!({
                "type": "object",
                "properties": {
                    "lines": {
                        "type": "integer",
                        "description": "How many trailing lines to return (1-2000, default 200)."
                    }
                },
                "additionalProperties": false
            }),
        ),
        ToolSpec::new(
            "find_mod_conflicts",
            "List mods in the current pack that declare conflicting or incompatible versions, \
             including the mods named in the loader's own resolution errors.",
            no_args(),
        ),
        ToolSpec::new(
            "list_mods",
            "List every mod in the current pack with its id, version, loader side and enabled state.",
            no_args(),
        ),
        ToolSpec::new(
            "check_mod_updates",
            "Check installed mods against their providers and classify each available update as \
             safe or risky. Read-only: nothing is updated.",
            json!({
                "type": "object",
                "properties": {
                    "only_outdated": {
                        "type": "boolean",
                        "description": "Return only mods with an available update (default true)."
                    }
                },
                "additionalProperties": false
            }),
        ),
        ToolSpec::new(
            "search_content",
            "Search Modrinth and CurseForge for mods, resource packs, shaders or datapacks \
             compatible with the current pack.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "kind": {
                        "type": "string",
                        "enum": ["mod", "resourcepack", "shader", "datapack"],
                        "description": "Content kind to search (default mod)."
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ),
        ToolSpec::new(
            "list_worlds",
            "List the worlds in the current pack's instance, with version and last-played time.",
            no_args(),
        ),
        ToolSpec::new(
            "read_config_file",
            "Read a configuration file from the current pack (path is relative to the instance).",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Instance-relative path, e.g. config/sodium-options.json"
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        ToolSpec::new(
            "java_status",
            "Report the Java runtimes available on this machine and which one the current pack needs.",
            no_args(),
        ),
        ToolSpec::new(
            "project_summary",
            "Summarise the open pack: name, version, Minecraft and loader, and how much content of \
             each kind it carries.",
            no_args(),
        ),
        ToolSpec::new(
            "list_content",
            "List the resource packs or shader packs in the instance and whether each is enabled.",
            json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["resourcepacks", "shaderpacks"],
                        "description": "Which pack folder to list (default resourcepacks)."
                    }
                },
                "additionalProperties": false
            }),
        ),
        ToolSpec::new(
            "read_options_file",
            "Read a key=value file from the instance (options.txt, server.properties, a config \
             .properties, …), optionally only the keys you ask for.",
            json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Instance-relative path (default options.txt)."
                    },
                    "keys": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Only return these keys, if given."
                    }
                },
                "additionalProperties": false
            }),
        ),
        ToolSpec::new(
            "search_local_files",
            "Find files inside the instance by name substring, to locate a config, a log or a jar.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Substring of the file name." },
                    "extension": {
                        "type": "string",
                        "description": "Only files with this extension (e.g. json, jar, toml)."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum hits to return (1-200, default 50)."
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ),
        ToolSpec::new(
            "list_crash_reports",
            "List the crash reports the current pack has produced.",
            no_args(),
        ),
        ToolSpec::new(
            "read_crash_report",
            "Analyse one crash report: the detected cause, the mods it names and the suggested fixes.",
            json!({
                "type": "object",
                "properties": {
                    "report_id": {
                        "type": "string",
                        "description": "Report id from list_crash_reports; omit for the newest."
                    }
                },
                "additionalProperties": false
            }),
        ),
        ToolSpec::new(
            "list_snapshots",
            "List the safety snapshots taken for this pack — what changed and when.",
            no_args(),
        ),
        ToolSpec::new(
            "recent_launches",
            "Recent launch history: when the game started, how it ended and how long it ran.",
            json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "How many entries (1-100, default 20)." }
                },
                "additionalProperties": false
            }),
        ),
        ToolSpec::new(
            "list_servers",
            "List the multiplayer servers saved in the instance. Read-only: nothing is pinged or changed.",
            no_args(),
        ),
        ToolSpec::new(
            "world_dimensions",
            "List the dimensions that exist in one world of the instance.",
            json!({
                "type": "object",
                "properties": {
                    "world": { "type": "string", "description": "World folder name from list_worlds." }
                },
                "required": ["world"],
                "additionalProperties": false
            }),
        ),
        ToolSpec::new(
            "disk_usage",
            "Report how much disk the instance and its mods take, so a size problem is visible.",
            no_args(),
        ),
        ToolSpec::new(
            "mod_file_metadata",
            "Read what a mod jar in the instance declares: its mod id, version, loader and \
             dependencies, straight from the jar's metadata.",
            json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "Jar file name inside the instance mods folder (e.g. sodium.jar)."
                    }
                },
                "required": ["file"],
                "additionalProperties": false
            }),
        ),
        ToolSpec::new(
            "lookup_modrinth_project",
            "Look up a Modrinth project by id or slug: description, loader/version support and links. \
             Read-only: nothing is installed.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Project id or slug." }
                },
                "required": ["id"],
                "additionalProperties": false
            }),
        ),
    ]
}

/// `tools` payload for an OpenAI-compatible request.
pub fn to_openai_tools(tools: &[ToolSpec]) -> Value {
    Value::Array(tools.iter().map(ToolSpec::to_openai).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn names_are_unique_and_descriptions_are_useful() {
        let tools = tool_catalogue();
        assert!(tools.len() >= 8, "catalogue shrank unexpectedly");
        let names: HashSet<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
        assert_eq!(names.len(), tools.len(), "duplicate tool name");
        for tool in &tools {
            assert!(
                tool.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "tool names must be snake_case: {}",
                tool.name
            );
            assert!(
                tool.description.len() > 30,
                "tool {} needs a real description",
                tool.name
            );
        }
    }

    #[test]
    fn every_schema_is_an_object_and_required_fields_exist() {
        for tool in tool_catalogue() {
            assert_eq!(
                tool.parameters["type"], "object",
                "{} must take an object",
                tool.name
            );
            let properties = tool.parameters["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("{} has no properties object", tool.name));
            if let Some(required) = tool.parameters["required"].as_array() {
                for field in required {
                    let field = field.as_str().expect("required entries are strings");
                    assert!(
                        properties.contains_key(field),
                        "{} requires unknown property {field}",
                        tool.name
                    );
                }
            }
        }
    }

    #[test]
    fn openai_payload_wraps_each_tool_as_a_function() {
        let tools = tool_catalogue();
        let payload = to_openai_tools(&tools);
        let entries = payload.as_array().expect("array");
        assert_eq!(entries.len(), tools.len());
        for entry in entries {
            assert_eq!(entry["type"], "function");
            assert!(entry["function"]["name"].is_string());
            assert_eq!(entry["function"]["parameters"]["type"], "object");
        }
    }
}
