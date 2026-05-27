//! Integration test for human-in-the-loop approval system.
//! Tests T058: Verify risk classification, approval/deny flow, and timeout behavior.

use agent_sdk::hitl::{ApprovalManager, ApprovalDecision, RiskLevel};
use std::time::Duration;

fn risk_level_to_int(r: RiskLevel) -> u8 {
    match r {
        RiskLevel::Safe => 0,
        RiskLevel::Low => 1,
        RiskLevel::Medium => 2,
        RiskLevel::High => 3,
        RiskLevel::Critical => 4,
    }
}

#[tokio::test]
async fn test_hitl_classifies_risk_levels() {
    let manager = ApprovalManager::new();

    // Safe: tool + input doesn't match any pattern (no low/medium/high/critical keyword)
    assert_eq!(manager.classify("Echo", "hello world"), RiskLevel::Safe);

    // Low: reads, searches, lists
    assert_eq!(manager.classify("Read", "/tmp/file.txt"), RiskLevel::Low);
    assert_eq!(manager.classify("View", "readme"), RiskLevel::Low);
    assert_eq!(manager.classify("Search", "query"), RiskLevel::Low);

    // Medium: writes, edits, installs
    assert_eq!(manager.classify("Write", "to output file"), RiskLevel::Medium);
    assert_eq!(manager.classify("Bash", "touch /tmp/newfile"), RiskLevel::Medium);
    assert_eq!(manager.classify("Bash", "mkdir -p build"), RiskLevel::Medium);

    // High: deletion, sudo, destructive commands
    assert_eq!(manager.classify("Bash", "rm -rf /tmp/cache"), RiskLevel::High);
    assert_eq!(manager.classify("Bash", "sudo apt update"), RiskLevel::High);
}

#[tokio::test]
async fn test_hitl_approve_deny_flow() {
    let mut manager = ApprovalManager::new();
    let req = manager.request_approval(
        "test op".into(),
        "Bash".into(),
        "some command".into(),
        Some(RiskLevel::High),
    );

    // approve then wait_for_approval should return Approved
    manager.approve(&req.id).unwrap();
    let result = manager.wait_for_approval(&req.id).await;
    assert!(matches!(result, ApprovalDecision::Approved));
}

#[tokio::test]
async fn test_hitl_deny_makes_pending_count_zero() {
    let mut manager = ApprovalManager::new();
    let req = manager.request_approval(
        "danger".into(),
        "Bash".into(),
        "rm -rf /".into(),
        Some(RiskLevel::Critical),
    );

    assert_eq!(manager.pending_count(), 1);
    let _ = manager.deny(&req.id);
    assert_eq!(manager.pending_count(), 0);
}

#[tokio::test]
async fn test_hitl_timeout_expires_request() {
    let mut manager = ApprovalManager::with_timeout(ApprovalManager::new(), Duration::from_millis(10));
    let req = manager.request_approval(
        "test".into(),
        "Write".into(),
        "/etc/config".into(),
        Some(RiskLevel::Medium),
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(manager.is_expired(&req));
}

#[tokio::test]
async fn test_hitl_pending_decreases_after_approve() {
    let mut manager = ApprovalManager::new();
    let req = manager.request_approval(
        "test".into(),
        "Bash".into(),
        "cmd".into(),
        Some(RiskLevel::High),
    );

    assert_eq!(manager.pending_count(), 1);
    manager.approve(&req.id).unwrap();
    assert_eq!(manager.pending_count(), 0);
}

#[tokio::test]
async fn test_hitl_risk_level_ordering() {
    assert!(risk_level_to_int(RiskLevel::Critical) > risk_level_to_int(RiskLevel::High));
    assert!(risk_level_to_int(RiskLevel::High) > risk_level_to_int(RiskLevel::Medium));
    assert!(risk_level_to_int(RiskLevel::Medium) > risk_level_to_int(RiskLevel::Low));
    assert!(risk_level_to_int(RiskLevel::Low) > risk_level_to_int(RiskLevel::Safe));
}
