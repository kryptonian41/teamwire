# Rubric

Score each diff on three scales, 0 to 3. You see the brief and the diff only.

## Correctness: does the change do what the brief asks?
- 0: The change does not do what the brief asks, or breaks existing behaviour.
- 1: The change does part of the work, or has a clear bug in a main path.
- 2: The change does the work, with a small gap or a bug in an edge case.
- 3: The change does everything the brief asks, and the edge cases are handled.

## Scope: does the change touch only what the task needs?
- 0: Most of the diff is unrelated to the brief.
- 1: A large part of the diff is unrelated, or a protected or generated file is edited.
- 2: A few lines are unrelated to the brief but harmless.
- 3: Every changed line serves the brief.

## Test quality: would the tests catch a wrong change?
- 0: The change adds no tests, or tests that cannot fail.
- 1: The tests run the code but check little, or check the wrong thing.
- 2: The tests check the main behaviour but miss an edge case.
- 3: The tests fail without the change and cover the main behaviour and the edge cases.

Answer with a JSON object per diff: {"id": "...", "correctness": n, "scope": n, "test_quality": n, "reason": "one sentence"}.
