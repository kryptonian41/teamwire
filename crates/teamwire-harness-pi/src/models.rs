//! The models pi can run, read from `pi --list-models`.
//!
//! pi lists only the models of a provider it found a credential for. So the
//! list shows what this machine can reach, though a listed model can still
//! fail at the provider (a denied cloud policy, for one). `exclude` drops a
//! provider the user never wants teamwire to offer.

use std::time::Duration;

use teamwire_core::{
    CatalogSource, Command, HarnessError, HarnessErrorCode, ModelCatalog, ModelInfo, Platform,
};

/// The values of `pi --thinking`, lowest first (`pi --help`, pi 1.4.0).
const EFFORTS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

pub(crate) fn list(
    platform: &dyn Platform,
    pi_bin: &str,
    exclude: &[String],
    timeout: Duration,
) -> Result<ModelCatalog, HarnessError> {
    let cmd = Command::new(pi_bin).args(["--list-models"]);
    let out = platform
        .run(&cmd, timeout)
        .map_err(|e| HarnessError::new(HarnessErrorCode::Io, format!("`pi --list-models`: {e}")))?;
    if out.timed_out {
        return Err(HarnessError::new(
            HarnessErrorCode::Timeout,
            format!("`pi --list-models` did not finish within {timeout:?}"),
        ));
    }
    if out.exit_code != 0 {
        return Err(HarnessError::new(
            HarnessErrorCode::Other,
            format!("`pi --list-models` failed: {}", out.failure_text()),
        ));
    }
    let mut models = parse(&String::from_utf8_lossy(&out.stdout))?;
    models.retain(|m| {
        m.provider
            .as_ref()
            .is_none_or(|provider| !exclude.contains(provider))
    });
    Ok(ModelCatalog {
        source: CatalogSource::Listed,
        models,
        efforts: EFFORTS.map(str::to_owned).to_vec(),
    })
}

/// The table `pi --list-models` prints: a header line, then one row per
/// model. The header names the columns, so a reordered table still parses
/// and a table without the columns this reads is refused.
fn parse(table: &str) -> Result<Vec<ModelInfo>, HarnessError> {
    let bad = |detail: String| HarnessError::new(HarnessErrorCode::BadInput, detail);
    let mut lines = table.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<&str> = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    let column = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .ok_or_else(|| bad(format!("`pi --list-models` has no `{name}` column")))
    };
    let (provider, model, context, thinking) = (
        column("provider")?,
        column("model")?,
        column("context")?,
        column("thinking")?,
    );
    lines
        .map(|line| {
            let cells: Vec<&str> = line.split_whitespace().collect();
            let cell = |i: usize| {
                cells
                    .get(i)
                    .copied()
                    .ok_or_else(|| bad(format!("`pi --list-models` row is short: {line}")))
            };
            Ok(ModelInfo {
                id: cell(model)?.to_owned(),
                aliases: Vec::new(),
                provider: Some(cell(provider)?.to_owned()),
                context_tokens: tokens(cell(context)?)
                    .ok_or_else(|| bad(format!("`pi --list-models` context is odd: {line}")))?,
                thinking: match cell(thinking)? {
                    "yes" => true,
                    "no" => false,
                    _ => return Err(bad(format!("`pi --list-models` thinking is odd: {line}"))),
                },
            })
        })
        .collect()
}

/// `1M`, `1.0M`, `200K` or `163.8K` as a token count. The outer `None` is a
/// value that is not a count; `Some(None)` is a `-`, for no figure.
fn tokens(text: &str) -> Option<Option<u64>> {
    if text == "-" {
        return Some(None);
    }
    let (number, unit) = match text.char_indices().last()? {
        (i, 'K') => (&text[..i], 1_000),
        (i, 'M') => (&text[..i], 1_000_000),
        _ => (text, 1),
    };
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    let digits = u32::try_from(fraction.len()).ok()?;
    let fraction_part = match fraction {
        "" => 0,
        f => f.parse::<u64>().ok()?.checked_mul(unit)? / 10u64.checked_pow(digits)?,
    };
    let whole_part = whole.parse::<u64>().ok()?.checked_mul(unit)?;
    whole_part.checked_add(fraction_part).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_read_the_unit_and_the_decimal() {
        assert_eq!(tokens("1M"), Some(Some(1_000_000)));
        assert_eq!(tokens("1.0M"), Some(Some(1_000_000)));
        assert_eq!(tokens("200K"), Some(Some(200_000)));
        assert_eq!(tokens("163.8K"), Some(Some(163_800)));
        assert_eq!(tokens("122.9K"), Some(Some(122_900)));
        assert_eq!(tokens("4096"), Some(Some(4096)));
        assert_eq!(tokens("-"), Some(None));
        assert_eq!(tokens("lots"), None);
        assert_eq!(tokens("K"), None);
    }
}
