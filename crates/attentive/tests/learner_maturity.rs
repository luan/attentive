use attentive_learn::Learner;

#[test]
fn test_learner_new_is_observing() {
    let learner = Learner::new();
    assert_eq!(
        learner.boost_weight(),
        0.0,
        "New learner should be Observing with 0.0 boost"
    );
}

#[test]
fn test_learner_serialization() {
    let learner = Learner::new();

    let json = serde_json::to_string(&learner).unwrap();
    let loaded: Learner = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.boost_weight(), learner.boost_weight());
}

#[test]
fn test_learner_maturity_transition() {
    // Test via serialization to set turn_count
    let json_observing = r#"{"turn_count":10,"prompt_file_affinity":{},"maturity":"observing"}"#;
    let mut learner: Learner = serde_json::from_str(json_observing).unwrap();
    learner.update_maturity();
    assert_eq!(
        learner.boost_weight(),
        0.0,
        "turn_count < 25 should be Observing"
    );

    let json_active = r#"{"turn_count":25,"prompt_file_affinity":{},"maturity":"observing"}"#;
    let mut learner: Learner = serde_json::from_str(json_active).unwrap();
    learner.update_maturity();
    assert_eq!(
        learner.boost_weight(),
        0.35,
        "turn_count >= 25 should be Active"
    );
}
