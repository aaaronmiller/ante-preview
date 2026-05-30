//! Integration tests for sub-agent hook dispatch.
//!
//! Validates that:
//! - A `HookDefinition::SubAgent` in a `HookMatchRule` executes the callback
//! - The callback receives correct (agent_name, formatted_task)
//! - The callback's JSON result is parsed as a `HookDecision`
//! - Errors produce `Allow` decisions (non-blocking)
//! - Wrong event type → no callback invocation

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agent_sdk::event::EventBus;
use agent_sdk::hooks::InvokeSubAgent;
use agent_sdk::hooks::registry::HookRegistry;
use ante_protocol_shape::{
    BasePayload, EventPayload, EventType,
    SubAgentPayload,
};
use ante_protocol_shape::settings::{HookDefinition, HookMatchRule};

fn make_bp() -> BasePayload {
    BasePayload::new("/tmp".into(), "0.1.0-test".into())
}

fn make_rule(event_types: Vec<EventType>, hooks: Vec<HookDefinition>) -> HookMatchRule {
    HookMatchRule {
        event_types,
        tool_name_pattern: None,
        hooks,
    }
}

#[tokio::test]
async fn test_subagent_hook_callback_invoked() {
    let bp = make_bp();
    let invoked = Arc::new(AtomicBool::new(false));
    let invoked_clone = invoked.clone();

    let callback: InvokeSubAgent = Arc::new(move |agent_name, task| {
        invoked_clone.store(true, Ordering::SeqCst);
        assert_eq!(agent_name, "test-agent", "agent name should match");
        assert!(task.contains("prove"), "task should contain event data");
        Box::pin(async move {
            Ok(serde_json::json!({"decision": "allow"}))
        })
    });

    let rule = make_rule(
        vec![EventType::AntePreSubAgentDispatch],
        vec![HookDefinition::SubAgent {
            agent_name: "test-agent".into(),
            task: "execute: {event.subagent_name}: prove this works".into(),
        }],
    );
    let registry = HookRegistry::new(vec![rule]);
    let bus = EventBus::new(registry)
        .with_subagent_callback(callback);

    let payload = EventPayload::AntePreSubAgentDispatch(SubAgentPayload {
        base: bp,
        subagent_name: "test-agent".into(),
        task: "prove this works".into(),
        model: None,
        result: None,
    });

    let result = bus.emit(&payload).await;

    assert!(invoked.load(Ordering::SeqCst), "callback should have been invoked");
    assert!(result.decision.is_allowed(), "sub-agent hook should allow");
}

#[tokio::test]
async fn test_subagent_hook_not_invoked_for_wrong_event() {
    let bp = make_bp();
    let invoked = Arc::new(AtomicBool::new(false));
    let invoked_clone = invoked.clone();

    let callback: InvokeSubAgent = Arc::new(move |_agent, _task| {
        invoked_clone.store(true, Ordering::SeqCst);
        Box::pin(async move {
            Ok(serde_json::json!({"decision": "allow"}))
        })
    });

    // Rule only matches PermissionRequest, not AntePreSubAgentDispatch
    let rule = make_rule(
        vec![EventType::PermissionRequest],
        vec![HookDefinition::SubAgent {
            agent_name: "sa".into(),
            task: "x".into(),
        }],
    );
    let registry = HookRegistry::new(vec![rule]);
    let bus = EventBus::new(registry)
        .with_subagent_callback(callback);

    let payload = EventPayload::AntePreSubAgentDispatch(SubAgentPayload {
        base: bp,
        subagent_name: "sa".into(),
        task: "should not match".into(),
        model: None,
        result: None,
    });

    let result = bus.emit(&payload).await;

    assert!(!invoked.load(Ordering::SeqCst), "callback NOT invoked for wrong event type");
    assert!(result.decision.is_allowed(), "should be allowed (no matching rule)");
}

#[tokio::test]
async fn test_subagent_hook_error_does_not_block() {
    let bp = make_bp();

    let callback: InvokeSubAgent = Arc::new(|_agent, _task| {
        Box::pin(async move {
            Err("intentional sub-agent failure".into())
        })
    });

    let rule = make_rule(
        vec![EventType::AntePreSubAgentDispatch],
        vec![HookDefinition::SubAgent {
            agent_name: "failing".into(),
            task: "fail".into(),
        }],
    );
    let registry = HookRegistry::new(vec![rule]);
    let bus = EventBus::new(registry)
        .with_subagent_callback(callback);

    let payload = EventPayload::AntePreSubAgentDispatch(SubAgentPayload {
        base: bp,
        subagent_name: "failing".into(),
        task: "do something".into(),
        model: None,
        result: None,
    });

    let result = bus.emit(&payload).await;

    // Errors in sub-agent hooks are logged but don't block the pipeline
    assert!(result.decision.is_allowed(), "errors should not block");
}

#[tokio::test]
async fn test_subagent_hook_returns_block_decision() {
    let bp = make_bp();

    let callback: InvokeSubAgent = Arc::new(|_agent, _task| {
        Box::pin(async move {
            Ok(serde_json::json!({"type": "deny", "reason": "cannot proceed"}))
        })
    });

    let rule = make_rule(
        vec![EventType::AntePreSubAgentDispatch],
        vec![HookDefinition::SubAgent {
            agent_name: "blocker".into(),
            task: "block everything".into(),
        }],
    );
    let registry = HookRegistry::new(vec![rule]);
    let bus = EventBus::new(registry)
        .with_subagent_callback(callback);

    let payload = EventPayload::AntePreSubAgentDispatch(SubAgentPayload {
        base: bp,
        subagent_name: "blocker".into(),
        task: "do something dangerous".into(),
        model: None,
        result: None,
    });

    let result = bus.emit(&payload).await;

    assert!(!result.decision.is_allowed(), "sub-agent hook returning deny should block");
}

#[tokio::test]
async fn test_subagent_hook_with_command_hook_chain() {
    let bp = make_bp();
    let invoked_subagent = Arc::new(AtomicBool::new(false));
    let invoked_subagent_clone = invoked_subagent.clone();

    let subagent_cb: InvokeSubAgent = Arc::new(move |_agent, _task| {
        invoked_subagent_clone.store(true, Ordering::SeqCst);
        Box::pin(async move {
            Ok(serde_json::json!({"decision": "allow"}))
        })
    });

    // Single rule with two hooks: SubAgent + Command
    let rule = make_rule(
        vec![EventType::AntePreSubAgentDispatch],
        vec![
            HookDefinition::SubAgent {
                agent_name: "sa".into(),
                task: "work".into(),
            },
            HookDefinition::Command {
                command: "echo".into(),
                args: vec!["hook-ok".into()],
                timeout_ms: None,
            },
        ],
    );
    let registry = HookRegistry::new(vec![rule]);
    let bus = EventBus::new(registry)
        .with_subagent_callback(subagent_cb);

    let payload = EventPayload::AntePreSubAgentDispatch(SubAgentPayload {
        base: bp,
        subagent_name: "sa".into(),
        task: "work".into(),
        model: None,
        result: None,
    });

    let result = bus.emit(&payload).await;

    assert!(invoked_subagent.load(Ordering::SeqCst), "subagent callback invoked");
    assert!(result.decision.is_allowed(), "both hooks allowed");
}
