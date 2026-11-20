//! Version comparison for the home's compatibility contract (plan F11).
//!
//! Deliberately small: it parses the numeric release out of a tag (`v10.5`, `10.5.0`,
//! `0.1.0-alpha.3`) and ignores any pre-release or build suffix. Anything it cannot parse is
//! `Unknown`, and `Unknown` never blocks — an unparseable value is a reason to carry on and say
//! so, not a reason to refuse to start.

/// A parsed numeric release, without any pre-release or build suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Release {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Release {
    /// Parses `v10.5`, `10.5`, `10.5.1`, `0.1.0-alpha.3`. Returns `None` for anything else —
    /// including a moving ref such as `main`, and a value with a non-numeric component.
    pub fn parse(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        let body = trimmed
            .strip_prefix('v')
            .or_else(|| trimmed.strip_prefix('V'))
            .unwrap_or(trimmed);
        // A pre-release or build suffix does not change which release this is.
        let core = body.split(['-', '+']).next().unwrap_or(body);
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().unwrap_or("0").parse().ok()?;
        let patch = parts.next().unwrap_or("0").parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

/// Whether `reference` is a moving ref rather than a release. A floating ref (`main`) can never
/// be behind anything, so it is never drift.
pub fn floats(reference: &str) -> bool {
    Release::parse(reference).is_none()
}

/// A running version compared against a declared minimum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compatibility {
    Satisfied,
    TooOld,
    /// One side (or both) could not be parsed. Callers must carry on; some compatibility
    /// contracts cannot be checked from a prose value.
    Unknown,
}

/// `current` against the minimum `required`. An unparseable side is [`Compatibility::Unknown`].
pub fn check_minimum(current: &str, required: &str) -> Compatibility {
    match (Release::parse(current), Release::parse(required)) {
        (Some(current), Some(required)) => {
            if current >= required {
                Compatibility::Satisfied
            } else {
                Compatibility::TooOld
            }
        }
        _ => Compatibility::Unknown,
    }
}

/// How many of `candidates` are newer than `pinned`, and the newest of them. `None` when the pin
/// is a moving ref (`main`) or nothing is newer — so a floating pin never reports drift.
pub fn behind(pinned: &str, candidates: &[String]) -> Option<(usize, String)> {
    let pin = Release::parse(pinned)?;
    let mut newer: Vec<(Release, &str)> = candidates
        .iter()
        .filter_map(|candidate| {
            Release::parse(candidate).map(|release| (release, candidate.as_str()))
        })
        .filter(|(release, _)| *release > pin)
        .collect();
    if newer.is_empty() {
        return None;
    }
    newer.sort_by_key(|(release, _)| *release);
    let (_, newest) = *newer.last()?;
    Some((newer.len(), newest.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidates(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_owned()).collect()
    }

    #[test]
    fn parses_releases_with_or_without_a_prefix_and_suffix() {
        assert_eq!(
            Release::parse("v10.5"),
            Some(Release {
                major: 10,
                minor: 5,
                patch: 0
            })
        );
        assert_eq!(Release::parse("10.5.1").unwrap().patch, 1);
        // A pre-release suffix does not change which release this is.
        assert_eq!(Release::parse("0.1.0-alpha.3"), Release::parse("0.1.0"));
        // Two components are enough; a fourth is not a release.
        assert_eq!(Release::parse("11").unwrap().minor, 0);
        assert!(Release::parse("10.5.0.1").is_none());
    }

    #[test]
    fn an_unparseable_value_is_never_treated_as_a_release() {
        for value in ["main", "HEAD", "master", "", "v", "latest", "release-10"] {
            assert!(Release::parse(value).is_none(), "{value}");
            assert!(floats(value), "{value}");
        }
    }

    #[test]
    fn minimum_check_orders_versions_and_stays_unknown_on_junk() {
        assert_eq!(check_minimum("v10.5", "10.4"), Compatibility::Satisfied);
        assert_eq!(check_minimum("10.4", "v10.5"), Compatibility::TooOld);
        assert_eq!(check_minimum("10.5", "10.5"), Compatibility::Satisfied);
        assert_eq!(
            check_minimum("0.1.0-alpha.3", "0.1.0"),
            Compatibility::Satisfied
        );
        // Unknown must never be mistaken for a failure in either direction.
        assert_eq!(check_minimum("main", "10.5"), Compatibility::Unknown);
        assert_eq!(check_minimum("0.1.0", "main"), Compatibility::Unknown);
        assert_eq!(check_minimum("latest", "newest"), Compatibility::Unknown);
    }

    #[test]
    fn a_floating_pin_is_never_behind_anything() {
        let tags = candidates(&["v10.4", "v10.5", "v10.8"]);
        assert_eq!(behind("main", &tags), None);
        assert_eq!(behind("v10.5", &tags), Some((1, "v10.8".to_owned())));
        assert_eq!(
            behind("v10.4", &tags),
            Some((2, "v10.8".to_owned())),
            "counts every newer release, not only the newest"
        );
        assert_eq!(behind("v10.8", &tags), None, "up to date is not drift");
        assert_eq!(behind("v10.5", &[]), None, "no tags is not drift");
    }
}
