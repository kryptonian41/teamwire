# ADR 0056: Code works out a model's family to label it and to warn

Status: accepted, 2026-11-20. Adds to ADR 0053, which said "No program
behaviour changes". The rest of ADR 0053 stands: the lead sets each
member's `harness` from the model's family, and teamwire never picks a
harness from a model id. ADR 0033 stands too: teamwire names no model and
no best model.

## Context

A model's family is its vendor line, such as Claude, GPT or Gemini. Under
ADR 0053 the lead reads the family by eye from the model string. Two jobs
now need the program to read it.

- **Sets.** A set is two or three members that do the same judging job on
  one brief, each on a model of a different family. A set of two GPT models
  gives the lead less than the lead thinks. Launch must be able to warn
  about it, so the program must tell the families apart.
- **`teamwire models`.** pi lists hundreds of models through OpenRouter and
  other providers. The lead picks a model of another family faster when the
  list says the family beside each model.

The family is not always in the model string. Claude Code's alias `opus`
names no vendor, yet it is a Claude model. pi's `openrouter/openai/o3`
names its vendor only in the middle part. The core crate also has a rule
of its own: it never names a product and never matches a harness id
(`crates/teamwire-core/src/lib.rs`, `names.rs`).

## Decision

1. **The core reads a family from a model string alone.**
   `teamwire_core::model_family(model)` returns a `ModelFamily`:
   `claude`, `gpt`, `gemini`, `gemma`, `grok`, `deepseek`, `kimi`, `glm`,
   `qwen`, `mistral`, `llama` or `unknown`. The function lowercases the
   string. It places an `openrouter/<vendor>/<id>` string by its vendor
   part alone, after a leading `~`. Google's vendor part is the one
   exception, because Google makes two lines: an id that starts with
   `gemini` is `gemini`, one that starts with `gemma` is `gemma`, and any
   other Google id is `unknown`. Gemma is its own family, not Gemini, so
   a Gemini reader and a Gemma reader in one set count as two families.
   Any other string is placed by the start of its `normalise_model` name,
   after the provider and region words are gone.
2. **The tables name vendors and the first word of their ids, never a
   model.** A vendor's next model needs no change. A string the tables do
   not place is `unknown`. The function never guesses.
3. **A harness answers for its own models.** The `Harness` trait gets a
   default method, `model_family(&self, model)`, that calls the core's
   function. The Claude Code adapter overrides it and returns `claude` for
   every value, because Claude Code starts Claude models only. pi keeps the
   default. So the core still matches no harness id, and the claude-code
   rule lives in the Claude Code adapter (`crates/teamwire-harness-claude`).
4. **`teamwire models` shows the family.** A `family` column follows the
   `model` column, and each model object in `--json` has a `family`
   string. The command asks the harness it lists, through the trait method,
   with the value a spec's `model` takes, such as
   `openrouter/openai/gpt-6.2-astra`. The bare id would lose the vendor part.
5. **A family labels and warns, and does nothing else.** No code picks a
   harness from a family (ADR 0053, Rejected). No code refuses a launch
   over a family. Whether launch should ever refuse is left open.

Known limit: on pi's list of 2026-11-20, Amazon Bedrock's Mistral models
whose id starts with `pixtral`, `ministral`, `magistral` or `voxtral`, and
Bedrock's `us-gov.openai.gpt-oss-*` models, stay `unknown`, because the
start table has no row for those words and `normalise_model` keeps a region
word that holds a hyphen.

## Rejected

- **The core matches the harness id `claude-code`.** This was the first
  plan. It breaks the core's rule of never naming a product, and a third
  harness of one family would need another match in the core.
- **A `family` field on `ModelInfo`.** Twelve places build that struct by
  hand, harness crates among them. Each would have to work out the family.
  The output that shows the family works it out instead.
- **A family flag in `HarnessCaps`.** `tests/fakes` builds that struct by
  hand, so every fake would change. One trait method with a default
  changes nothing for a harness that keeps it.
- **Guessing from a model name, such as `o3` for GPT.** A name the tables
  do not know is `unknown`, and launch then asks the lead to check the
  model by eye.

## Consequences

- A new vendor needs one row in each table of
  `crates/teamwire-core/src/models/family.rs`. Until then its models are
  `unknown`.
- A new harness whose every model is of one family overrides
  `Harness::model_family`, as Claude Code does.
- The tests in `crates/teamwire-core/src/models/family.rs` cover one model
  string per row of the agreed examples. The test
  `every_claude_code_model_is_of_the_claude_family` in
  `crates/teamwire-harness-claude/tests/models.rs` fails when Claude Code
  stops answering for its aliases. The test
  `the_family_column_is_the_harness_s_own_answer` in
  `crates/teamwire-cli/tests/models.rs` fails when `teamwire models` reads
  the string alone instead of asking the harness.
