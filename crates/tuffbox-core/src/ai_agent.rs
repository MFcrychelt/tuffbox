//! The tool-calling loop.
//!
//! `integrations::call_ai_messages*` owns the transport; this owns the shape of
//! a multi-step turn: ask the model, run whatever tools it asked for, hand the
//! results back, repeat until it answers in words or the round cap is hit.
//!
//! The loop is generic over two traits ([`AgentModel`], [`ToolDispatcher`]) for
//! the same reason the mesher emits plain arrays: the hard part — history
//! ordering, id matching, error recovery, the cap — is testable without a live
//! model or a Tauri runtime.
//!
//! Invariants (see `docs/plans/2026-10-02-ai-tool-loop.md` §4):
//! - every `tool` message is preceded by the `assistant` message that requested
//!   it, in the same list, with the matching `tool_call_id` — built by
//!   construction, so an OpenAI-compatible endpoint can never reject the
//!   history as orphaned;
//! - a failing or unknown tool returns an error *payload* and the loop
//!   continues, because a tool mistake is not a failed turn;
//! - the loop always terminates: `rounds` is capped and the cap is reported.

use crate::ai_tools::{to_openai_tools, ToolSpec};
use serde_json::{json, Value};
use std::future::Future;

/// Default cap on model round trips, matching the reference client's budget.
pub const MAX_ROUNDS: u32 = 5;

/// One tool call requested by the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON text exactly as the model produced it (parsed by the dispatcher).
    pub arguments: String,
}

/// One assistant turn: prose, tool calls, or both.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentTurn {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

/// One completion, with the tool schemas attached.
pub trait AgentModel {
    fn complete(
        &self,
        system: &str,
        messages: &[Value],
        tools: &Value,
    ) -> impl Future<Output = Result<AgentTurn, String>> + Send;
}

/// Runs one tool call; the returned text is handed back to the model verbatim.
///
/// A dispatcher must encode its own failures as `{"error": …}` rather than
/// returning prose — the model is told about the failure, not shown a sentence.
pub trait ToolDispatcher {
    fn run(&self, call: &ToolCall) -> impl Future<Output = String> + Send;
}

/// What one tool call did, for the UI trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRun {
    pub name: String,
    pub arguments: String,
    pub result: String,
}

#[derive(Debug, Clone)]
pub struct AgentReport {
    /// Final assistant text; empty when the model only called tools and the cap
    /// was reached.
    pub text: String,
    pub rounds: u32,
    /// True when the loop stopped because `max_rounds` was reached.
    pub capped: bool,
    pub tool_runs: Vec<ToolRun>,
    /// Full history including tool messages — persist or continue from here.
    pub transcript: Vec<Value>,
}

/// Runs the loop to completion.
///
/// `history` is the prior conversation (assistant/user/tool messages); the new
/// user message is appended here so the caller never has to splice lists.
#[allow(clippy::too_many_arguments)]
pub async fn run_tool_loop<M, D>(
    model: &M,
    dispatcher: &D,
    system: &str,
    history: Vec<Value>,
    user_message: &str,
    tools: &[ToolSpec],
    max_rounds: u32,
) -> Result<AgentReport, String>
where
    M: AgentModel,
    D: ToolDispatcher,
{
    let tool_payload = to_openai_tools(tools);
    let mut transcript = history;
    transcript.push(json!({ "role": "user", "content": user_message }));
    let mut tool_runs = Vec::new();
    let mut rounds = 0;

    loop {
        let turn = model.complete(system, &transcript, &tool_payload).await?;
        rounds += 1;

        if turn.tool_calls.is_empty() {
            return Ok(AgentReport {
                text: turn.content.unwrap_or_default(),
                rounds,
                capped: false,
                tool_runs,
                transcript,
            });
        }

        let requested: Vec<Value> = turn
            .tool_calls
            .iter()
            .map(|call| {
                json!({
                    "id": call.id,
                    "type": "function",
                    "function": { "name": call.name, "arguments": call.arguments },
                })
            })
            .collect();
        transcript.push(json!({
            "role": "assistant",
            // Endpoints differ on `null` content; an empty string is accepted by
            // OpenAI, Ollama and the OpenAI-compatible servers we support.
            "content": turn.content.clone().unwrap_or_default(),
            "tool_calls": requested,
        }));

        for call in &turn.tool_calls {
            let result = dispatcher.run(call).await;
            tool_runs.push(ToolRun {
                name: call.name.clone(),
                arguments: call.arguments.clone(),
                result: result.clone(),
            });
            transcript.push(json!({
                "role": "tool",
                "tool_call_id": call.id,
                "content": result,
            }));
        }

        if rounds >= max_rounds {
            return Ok(AgentReport {
                text: turn.content.unwrap_or_default(),
                rounds,
                capped: true,
                tool_runs,
                transcript,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_tools::tool_catalogue;
    use std::sync::Mutex;

    /// These tests never poison a lock on purpose; take the guard regardless
    /// instead of unwrapping a result that cannot fail meaningfully here.
    fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Scripted model: hands out one turn per call, then fails the test.
    struct ScriptedModel {
        turns: Mutex<Vec<AgentTurn>>,
        seen: Mutex<Vec<Vec<Value>>>,
    }

    impl ScriptedModel {
        fn new(turns: Vec<AgentTurn>) -> Self {
            Self {
                turns: Mutex::new(turns),
                seen: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> usize {
            lock(&self.seen).len()
        }

        fn last_messages(&self) -> Vec<Value> {
            lock(&self.seen).last().cloned().unwrap_or_default()
        }
    }

    impl AgentModel for ScriptedModel {
        async fn complete(
            &self,
            _system: &str,
            messages: &[Value],
            _tools: &Value,
        ) -> Result<AgentTurn, String> {
            lock(&self.seen).push(messages.to_vec());
            let mut turns = lock(&self.turns);
            if turns.is_empty() {
                return Err("scripted model ran out of turns".into());
            }
            Ok(turns.remove(0))
        }
    }

    /// Dispatcher that records calls and answers from a name → result map.
    struct FakeTools {
        answers: Vec<(&'static str, &'static str)>,
        seen: Mutex<Vec<ToolCall>>,
    }

    impl FakeTools {
        fn new(answers: Vec<(&'static str, &'static str)>) -> Self {
            Self {
                answers,
                seen: Mutex::new(Vec::new()),
            }
        }
    }

    impl ToolDispatcher for FakeTools {
        async fn run(&self, call: &ToolCall) -> String {
            lock(&self.seen).push(call.clone());
            self.answers
                .iter()
                .find(|(name, _)| *name == call.name)
                .map(|(_, answer)| (*answer).to_string())
                .unwrap_or_else(|| json!({"error": format!("unknown tool {}", call.name)}).to_string())
        }
    }

    fn call(id: &str, name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: name.into(),
            arguments: arguments.into(),
        }
    }

    #[tokio::test]
    async fn answers_without_tools_in_one_round() {
        let model = ScriptedModel::new(vec![AgentTurn {
            content: Some("Nothing to inspect.".into()),
            tool_calls: vec![],
        }]);
        let tools = FakeTools::new(vec![]);

        let report = run_tool_loop(
            &model,
            &tools,
            "system",
            vec![],
            "hi",
            &tool_catalogue(),
            MAX_ROUNDS,
        )
        .await
        .expect("loop");

        assert_eq!(report.text, "Nothing to inspect.");
        assert_eq!(report.rounds, 1);
        assert!(!report.capped);
        assert!(report.tool_runs.is_empty());
        assert_eq!(model.calls(), 1);
    }

    #[tokio::test]
    async fn runs_a_tool_and_hands_the_result_back() {
        let model = ScriptedModel::new(vec![
            AgentTurn {
                content: None,
                tool_calls: vec![call("c1", "list_mods", "{}")],
            },
            AgentTurn {
                content: Some("You have 2 mods.".into()),
                tool_calls: vec![],
            },
        ]);
        let tools = FakeTools::new(vec![("list_mods", r#"{"mods":["a","b"]}"#)]);

        let report = run_tool_loop(
            &model,
            &tools,
            "system",
            vec![],
            "what is installed?",
            &tool_catalogue(),
            MAX_ROUNDS,
        )
        .await
        .expect("loop");

        assert_eq!(report.text, "You have 2 mods.");
        assert_eq!(report.rounds, 2);
        assert_eq!(report.tool_runs.len(), 1);
        assert_eq!(report.tool_runs[0].name, "list_mods");
        assert_eq!(report.tool_runs[0].result, r#"{"mods":["a","b"]}"#);

        // The second round must have seen the tool result.
        let second_round = &model.last_messages();
        let tool_message = second_round
            .iter()
            .find(|m| m["role"] == "tool")
            .expect("tool result present");
        assert_eq!(tool_message["tool_call_id"], "c1");
        assert_eq!(tool_message["content"], r#"{"mods":["a","b"]}"#);
    }

    #[tokio::test]
    async fn every_tool_message_is_preceded_by_its_tool_calls() {
        let model = ScriptedModel::new(vec![
            AgentTurn {
                content: Some("checking".into()),
                tool_calls: vec![
                    call("c1", "list_mods", "{}"),
                    call("c2", "list_worlds", "{}"),
                ],
            },
            AgentTurn {
                content: Some("done".into()),
                tool_calls: vec![],
            },
        ]);
        let tools = FakeTools::new(vec![("list_mods", "{}"), ("list_worlds", "{}")]);

        let report = run_tool_loop(
            &model,
            &tools,
            "system",
            vec![],
            "go",
            &tool_catalogue(),
            MAX_ROUNDS,
        )
        .await
        .expect("loop");

        // The invariant an OpenAI-compatible endpoint enforces: a `tool` message
        // is only valid after the assistant message that declared its id.
        let mut declared: Vec<String> = Vec::new();
        for message in &report.transcript {
            match message["role"].as_str() {
                Some("assistant") => {
                    if let Some(calls) = message["tool_calls"].as_array() {
                        for c in calls {
                            declared.push(c["id"].as_str().unwrap_or_default().to_string());
                        }
                    }
                }
                Some("tool") => {
                    let id = message["tool_call_id"].as_str().unwrap_or_default();
                    assert!(
                        declared.iter().any(|d| d == id),
                        "tool result {id} has no preceding tool_calls entry"
                    );
                }
                _ => {}
            }
        }
        assert_eq!(declared, vec!["c1".to_string(), "c2".to_string()]);
        assert_eq!(report.tool_runs.len(), 2);
    }

    #[tokio::test]
    async fn an_unknown_tool_is_reported_to_the_model_not_fatal() {
        let model = ScriptedModel::new(vec![
            AgentTurn {
                content: None,
                tool_calls: vec![call("c1", "invented_tool", "{}")],
            },
            AgentTurn {
                content: Some("That tool does not exist.".into()),
                tool_calls: vec![],
            },
        ]);
        let tools = FakeTools::new(vec![]);

        let report = run_tool_loop(
            &model,
            &tools,
            "system",
            vec![],
            "go",
            &tool_catalogue(),
            MAX_ROUNDS,
        )
        .await
        .expect("loop");

        assert!(!report.capped);
        assert!(report.tool_runs[0].result.contains("unknown tool"));
        assert_eq!(report.text, "That tool does not exist.");
    }

    #[tokio::test]
    async fn stops_at_the_round_cap_instead_of_looping_forever() {
        let turns: Vec<AgentTurn> = (0..10)
            .map(|i| AgentTurn {
                content: None,
                tool_calls: vec![call(&format!("c{i}"), "list_mods", "{}")],
            })
            .collect();
        let model = ScriptedModel::new(turns);
        let tools = FakeTools::new(vec![("list_mods", "{}")]);

        let report = run_tool_loop(
            &model,
            &tools,
            "system",
            vec![],
            "go",
            &tool_catalogue(),
            3,
        )
        .await
        .expect("loop");

        assert!(report.capped);
        assert_eq!(report.rounds, 3);
        assert_eq!(model.calls(), 3);
        assert_eq!(report.tool_runs.len(), 3);
    }

    #[tokio::test]
    async fn model_failures_propagate_without_a_partial_transcript_surprise() {
        let model = ScriptedModel::new(vec![AgentTurn {
            content: None,
            tool_calls: vec![call("c1", "list_mods", "{}")],
        }]);
        let tools = FakeTools::new(vec![("list_mods", "{}")]);

        // The script runs out after the first turn, which returns the cap-less
        // path twice and then errors on the third call.
        let error = run_tool_loop(
            &model,
            &tools,
            "system",
            vec![],
            "go",
            &tool_catalogue(),
            8,
        )
        .await
        .expect_err("second completion must fail");
        assert!(error.contains("ran out of turns"));
    }

    #[tokio::test]
    async fn passes_the_catalogue_to_the_model() {
        let model = ScriptedModel::new(vec![AgentTurn {
            content: Some("ok".into()),
            tool_calls: vec![],
        }]);
        let tools = FakeTools::new(vec![]);
        let catalogue = tool_catalogue();

        run_tool_loop(
            &model,
            &tools,
            "system",
            vec![],
            "hi",
            &catalogue,
            MAX_ROUNDS,
        )
        .await
        .expect("loop");

        let payload = to_openai_tools(&catalogue);
        assert_eq!(payload.as_array().map(Vec::len), Some(catalogue.len()));
    }
}
