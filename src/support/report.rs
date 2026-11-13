//! Structured generation failures keep default diagnostics independent of input size.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

/// Complete failure details plus counts of affected top-level settings.
/// A setting counts once per reason, even when many nested values fail.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GenerationFailures {
    pub diagnostics: Vec<String>,
    pub settings: usize,
    pub empty_sources: usize,
    pub reasons: BTreeMap<&'static str, usize>,
}

impl GenerationFailures {
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub(super) fn reject(&mut self, path: &str, reason: &'static str, setting: bool) {
        self.push(vec![Issue::new(path, reason)], setting);
    }

    pub(super) fn push(&mut self, issues: Vec<Issue>, setting: bool) {
        debug_assert!(!issues.is_empty());
        if setting {
            self.settings += 1;
        } else {
            self.empty_sources += 1;
        }
        for reason in issues
            .iter()
            .map(|issue| issue.reason)
            .collect::<BTreeSet<_>>()
        {
            *self.reasons.entry(reason).or_default() += 1;
        }
        self.diagnostics.push(
            issues
                .into_iter()
                .map(|issue| issue.detail)
                .collect::<Vec<_>>()
                .join("; "),
        );
    }
}

impl fmt::Display for GenerationFailures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} unsupported settings", self.settings)?;
        if self.empty_sources > 0 {
            write!(f, "; {} unsupported empty sources", self.empty_sources)?;
        }
        f.write_char('.')?;
        let mut reasons = self.reasons.iter().collect::<Vec<_>>();
        reasons.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        const LIMIT: usize = 6;
        for (reason, count) in reasons.iter().take(LIMIT) {
            write!(f, "\n  {count}: {reason}")?;
        }
        if reasons.len() > LIMIT {
            write!(f, "\n  ... and {} more reasons", reasons.len() - LIMIT)?;
        }
        if self.reasons.values().sum::<usize>() > self.settings + self.empty_sources {
            f.write_str("\n  A setting or empty source can have more than one reason.")?;
        }
        Ok(())
    }
}

pub(super) struct Issue {
    reason: &'static str,
    detail: String,
}

impl Issue {
    pub(super) fn new(path: &str, reason: &'static str) -> Self {
        Self {
            reason,
            detail: format!("{path}: {reason}"),
        }
    }

    pub(super) fn with_detail(reason: &'static str, detail: String) -> Self {
        Self { reason, detail }
    }
}
