#![expect(
    clippy::indexing_slicing,
    reason = "a unit test indexes what it just built; a failure fails the test"
)]

use super::*;

fn one(report: &str) -> Block {
    let blocks = find_blocks(report);
    assert_eq!(blocks.len(), 1, "{blocks:?}");
    blocks.into_iter().next().unwrap()
}

#[test]
fn a_heading_block_runs_to_the_next_heading_of_the_same_level() {
    let b = one("# Report\n\n## Decisions I made\n\n- picked a\n- picked b\n\n## Notes\nmore\n");
    assert_eq!(b.heading, "## Decisions I made");
    assert_eq!(b.line, 3);
    assert_eq!(b.text, "- picked a\n- picked b");
}

#[test]
fn a_deeper_heading_stays_inside_a_heading_block() {
    let b = one("## Decisions I made\n- a\n### Why\nbecause\n## End\n");
    assert_eq!(b.text, "- a\n### Why\nbecause");
}

#[test]
fn a_heading_block_runs_to_the_end_of_the_file() {
    assert_eq!(one("## Decisions I made\n- a\n- b").text, "- a\n- b");
}

#[test]
fn a_numbered_heading_is_found() {
    let b = one("## 7. Decisions I made\n- a\n");
    assert_eq!(b.heading, "## 7. Decisions I made");
    assert_eq!(b.text, "- a");
}

#[test]
fn a_round_suffix_is_part_of_the_heading() {
    let blocks = find_blocks(
        "### Decisions I made (round 2)\n- a\n\n## Decisions I made (review round)\n- b\n",
    );
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].heading, "### Decisions I made (round 2)");
    assert_eq!(blocks[1].text, "- b");
}

#[test]
fn a_label_line_runs_to_the_end_of_its_list() {
    let b = one("Decisions I made:\n- a\n  continued\n- b\n\nNext paragraph.\n");
    assert_eq!(b.heading, "Decisions I made:");
    assert_eq!(b.text, "- a\n  continued\n- b");
}

#[test]
fn text_after_the_label_is_part_of_the_block() {
    let b = one("Decisions I made: none\n\nDone.\n");
    assert_eq!(b.heading, "Decisions I made:");
    assert_eq!(b.text, "none");
}

#[test]
fn an_inline_label_with_a_round_note_keeps_its_text() {
    let b = one("Decisions I made (this round): kept the old name\nNext line\n");
    assert_eq!(b.text, "kept the old name");
    assert_eq!(b.heading, "Decisions I made (this round):");
}

#[test]
fn a_bold_line_is_found_and_ends_at_the_next_bold_label() {
    let b = one("**Decisions I made**\n\n- a\n- b\n\n**Next label**\ntext\n");
    assert_eq!(b.heading, "**Decisions I made**");
    assert_eq!(b.text, "- a\n- b");
}

#[test]
fn a_bold_label_with_its_colon_inside_the_bold_is_found() {
    let b = one("**Decisions I made:** none\n");
    assert_eq!(b.heading, "**Decisions I made:**");
    assert_eq!(b.text, "none");
}

#[test]
fn the_heading_is_found_whatever_its_case() {
    assert_eq!(find_blocks("## DECISIONS I MADE\n- a\n").len(), 1);
}

#[test]
fn a_mention_in_the_middle_of_a_line_is_not_a_start() {
    let r = "Read first: AGENTS.md, the brief. Decisions I made\n- not a block\n";
    assert!(find_blocks(r).is_empty());
}

#[test]
fn a_report_holds_several_blocks_in_file_order() {
    let blocks = find_blocks("## Decisions I made\n- a\n## Fix\n## Decisions I made\n- b\n");
    let texts: Vec<_> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts, ["- a", "- b"]);
    assert_eq!(blocks[1].line, 4);
}

#[test]
fn a_hash_line_in_a_code_fence_does_not_end_a_block() {
    let b = one("## Decisions I made\n```\n# not a heading\n```\n- a\n");
    assert!(b.text.contains("# not a heading"), "{}", b.text);
    assert!(b.text.ends_with("- a"));
}

#[test]
fn a_report_without_the_label_has_no_blocks() {
    assert!(find_blocks("# Report\n- a\n").is_empty());
    assert!(find_blocks("").is_empty());
}

#[test]
fn an_indented_bold_line_continues_a_list_item() {
    let b =
        one("Decisions I made:\n1. Chose X.\n   **Why:** because Y.\n2. Chose Z.\n\n**Next**\n");
    assert_eq!(b.text, "1. Chose X.\n   **Why:** because Y.\n2. Chose Z.");
}

#[test]
fn a_line_indented_four_spaces_is_not_a_heading() {
    let b = one("## Decisions I made\n- Ran the build first:\n    # build\n- Chose Z.\n## Next\n");
    assert_eq!(b.text, "- Ran the build first:\n    # build\n- Chose Z.");
    let b = one("Decisions I made:\n- a\n    # not a heading\n- b\n");
    assert_eq!(b.text, "- a\n    # not a heading\n- b");
}

#[test]
fn a_heading_indented_up_to_three_spaces_still_ends_the_block() {
    let b = one("## Decisions I made\n- a\n   ## Next\n- b\n");
    assert_eq!(b.text, "- a");
}

#[test]
fn a_bare_label_takes_the_prose_right_under_it() {
    let b = one("Decisions I made:\nI kept the name.\n\nOther.\n");
    assert_eq!(b.text, "I kept the name.");
}
