use super::model_window;

#[test]
fn model_windows_follow_the_documented_table() {
    for (model, want) in [
        ("claude-opus-5-5", 1_000_000),
        ("claude-opus-4-7", 1_000_000),
        ("claude-opus-4-6", 200_000),
        ("claude-sonnet-4-6", 200_000),
        ("claude-sonnet-5", 1_000_000),
        ("claude-sonnet-5-5-20261001", 1_000_000),
        ("claude-haiku-5-5", 1_000_000),
        ("claude-haiku-4-5", 200_000),
        ("claude-fable-5-2", 1_000_000),
        ("claude-fable-5-1", 1_000_000),
        ("claude-opus-4-6[1m]", 1_000_000),
        ("opus", 1_000_000),
        ("claude-3-5-sonnet-20241022", 200_000),
        ("claude-3-7-sonnet-20250219", 200_000),
    ] {
        assert_eq!(model_window(model, false), Some(want), "{model}");
    }
    assert_eq!(model_window("gpt-5", false), None);
    assert_eq!(model_window("claude-opus-5-5", true), Some(200_000));
}
