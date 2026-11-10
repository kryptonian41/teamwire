---

description: "Task list for the wordtool `top` command"
---

# Tasks: Top Words Command

**Input**: Design documents from `/specs/001-top-words/`

**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/cli.md

**Tests**: Required. Principle II of the project constitution says every feature ships with `unittest` tests in the same change. Each story below writes its tests first and checks that they fail before the code is written.

**Organization**: Tasks are grouped by user story so each story can be built and tested on its own.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story the task belongs to (US1, US2, US3)
- Test command used throughout: `PYTHONPATH=src python3 -m unittest discover -s tests`

## Path Conventions

- Source: `src/wordtool/` (new module `top.py`, edit `__main__.py`)
- Tests: `tests/` (new file `test_top.py`)
- Standard library only. Do not install anything.

---

## Phase 1: Setup

**Purpose**: Confirm the project works before any change.

- [X] T001 Run `PYTHONPATH=src python3 -m unittest discover -s tests` from the repository root and confirm the 2 existing tests in tests/test_count.py pass

---

## Phase 2: Foundational

No blocking work. The layout (`src/wordtool/`, `tests/`) and the sub-command pattern in `src/wordtool/__main__.py` already exist, so each story can start right after Phase 1.

---

## Phase 3: User Story 1 - See the most frequent words in a file (Priority: P1) 🎯 MVP

**Goal**: `wordtool top FILE` prints the 10 most frequent words, one `<word> <count>` per line, highest count first.

**Independent Test**: Run `PYTHONPATH=src python3 -m wordtool top FILE` on a file with known counts and compare the lines with the expected list (see quickstart.md step 2).

### Tests for User Story 1 (write first, confirm they fail)

- [X] T002 [US1] In tests/test_top.py, add a `unittest.TestCase` for `top_words(text, n=10)` from `wordtool.top` with these cases: words come out highest count first (FR-003); words with equal counts come out alphabetically (FR-006); "The" and "the" are one word (FR-008); "end.", "(end)" and a curly-quoted word lose their edge punctuation (FR-008); the apostrophe in "don't" stays (FR-008); an underscore at the edge is stripped (FR-008); a piece that is only punctuation, such as "--", is not a word (FR-008); a text with fewer than n different words returns one pair per word (FR-010); empty text returns an empty list; with 12 different words and the default n, 10 pairs return (FR-004)
- [X] T003 [US1] In tests/test_top.py, add a second `unittest.TestCase` that writes a temporary UTF-8 file, calls `wordtool.__main__.main(["top", path])` with stdout captured by `contextlib.redirect_stdout`, and checks: lines are exactly `<word> <count>` with one space (FR-002); exactly 10 lines print for a file with 12 different words (FR-004); the return value is 0; an empty file prints nothing and returns 0; a file holding only whitespace prints nothing and returns 0
- [X] T004 [US1] Run `PYTHONPATH=src python3 -m unittest discover -s tests` and confirm the new tests fail (they fail because `wordtool.top` does not exist yet)

### Implementation for User Story 1

- [X] T005 [US1] Create src/wordtool/top.py with `top_words(text, n=10)`: split `text` on whitespace, lowercase each piece, strip leading and trailing characters that are not letters or digits with `re.sub(r"^[\W_]+|[\W_]+$", "", piece)`, drop empty results, count with `collections.Counter`, sort with `key=lambda item: (-item[1], item[0])`, and return the first `n` `(word, count)` pairs as a list (research.md items 1 to 3)
- [X] T006 [US1] In src/wordtool/__main__.py, import `top_words` from `wordtool.top`, add a `top` sub-command with a positional `path`, and in `main()` read the file as UTF-8 and print each pair as `<word> <count>` (follow the way `count` is wired)
- [X] T007 [US1] Run `PYTHONPATH=src python3 -m unittest discover -s tests` and confirm every test passes, including T002 and T003

**Checkpoint**: `top` works with the default N of 10. This is the MVP.

---

## Phase 4: User Story 2 - Choose how many words to see (Priority: P2)

**Goal**: `-n N` / `--number N` changes how many words print. A bad N is an error.

**Independent Test**: Run `top FILE -n 3` on a file with at least 5 different words and count the lines (quickstart.md step 3).

### Tests for User Story 2 (write first, confirm they fail)

- [X] T008 [US2] In tests/test_top.py, add a `unittest.TestCase` that calls `main()` with captured stdout and stderr (`contextlib.redirect_stdout`, `contextlib.redirect_stderr`) and checks: `["top", path, "-n", "3"]` prints exactly 3 lines, the 3 most frequent words (FR-005); `["top", path, "--number", "3"]` prints the same 3 lines; `-n 20` on a 4-word file prints 4 lines (FR-010); `-n abc`, `-n 0` and `-n -1` each raise `SystemExit` with a non-zero code, print nothing to stdout, and print something to stderr (FR-007, FR-011)
- [X] T009 [US2] Run `PYTHONPATH=src python3 -m unittest discover -s tests` and confirm these new tests fail before T010: `-n 3`, `--number 3` and `-n 20` (the `-n` flag is not known yet). The `-n abc`, `-n 0` and `-n -1` cases may already pass, because argparse rejects an unknown `-n` with exit code 2 and a message on stderr

### Implementation for User Story 2

- [X] T010 [US2] In src/wordtool/__main__.py, add `-n` / `--number` to the `top` sub-command with `type=int` and `default=10`, and pass it to `top_words`
- [X] T011 [US2] In src/wordtool/__main__.py, after `parse_args`, call `parser.error("N must be 1 or more")` when the command is `top` and the number is less than 1, so the exit code is 2 and stdout stays empty (research.md item 5)
- [X] T012 [US2] Run `PYTHONPATH=src python3 -m unittest discover -s tests` and confirm every test passes

**Checkpoint**: Stories 1 and 2 both work.

---

## Phase 5: User Story 3 - Get a clear error for a bad file (Priority: P3)

**Goal**: A missing, unreadable or non-UTF-8 file gives a message on stderr, an empty stdout and a non-zero exit code.

**Independent Test**: Run `top /tmp/missing.txt; echo $?` and check stderr, stdout and the exit code (quickstart.md step 4).

### Tests for User Story 3 (write first, confirm they fail)

- [X] T013 [US3] In tests/test_top.py, add a `unittest.TestCase` that calls `main()` with captured stdout and stderr and checks that each of these returns 1, prints nothing to stdout and prints a message to stderr (FR-011): a path that does not exist; a path that is a directory; a temporary file holding the bytes `b"\xff\xfe\xfa"`, which are not valid UTF-8
- [X] T014 [US3] Run `PYTHONPATH=src python3 -m unittest discover -s tests` and confirm the new tests fail (today `main()` raises an exception instead of returning 1)

### Implementation for User Story 3

- [X] T015 [US3] In src/wordtool/__main__.py, for the `top` command read the whole file and compute the ranking inside `try`, catch `OSError` and `UnicodeDecodeError`, print `wordtool: error: <reason>` to `sys.stderr`, and return 1; print the lines only after the `try` block succeeds so stdout stays empty on failure (research.md item 6). Leave the `count` command as it is
- [X] T016 [US3] Run `PYTHONPATH=src python3 -m unittest discover -s tests` and confirm every test passes

**Checkpoint**: All three stories work.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T017 Follow quickstart.md steps 2 to 4 by hand with `PYTHONPATH=src python3 -m wordtool top ...` and confirm the output and exit codes match
- [X] T018 Run `grep -n "^import\|^from" src/wordtool/top.py src/wordtool/__main__.py tests/test_top.py` and confirm every import is from the Python standard library or from `wordtool` (constitution Principle I)
- [X] T019 Run `PYTHONPATH=src python3 -m unittest discover -s tests` one last time and confirm it passes (constitution Principle II)

---

## Dependencies & Execution Order

- Phase 1 first, then Phase 3 (US1), Phase 4 (US2), Phase 5 (US3), then Phase 6.
- US2 and US3 need US1, because they extend the same sub-command in `src/wordtool/__main__.py`. US2 and US3 do not need each other, but they edit the same two files, so run them one after the other.
- Inside a story: tests first, then confirm they fail, then code, then confirm they pass.
- No task is marked `[P]`. Every task edits `tests/test_top.py` or `src/wordtool/__main__.py`, or runs after one that does.

## Implementation Strategy

- **MVP**: Phase 1 plus Phase 3 (T001 to T007). Stop and check `top FILE` against quickstart.md step 2.
- **Then**: add Phase 4 and Phase 5 in order, committing after each checkpoint.
- Commit after every checkpoint. Run the full test command from constitution Principle II before each commit.

---

## Phase 7: Convergence

- [X] T020 Normalise the text to Unicode NFC with `unicodedata.normalize("NFC", text)` in `top_words` in src/wordtool/top.py before it splits the text, and add a test in tests/test_top.py that a decomposed "café" (e + U+0301) and a precomposed "café" count as one word, per FR-008 (missing)
