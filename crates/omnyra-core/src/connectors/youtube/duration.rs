//! ISO-8601 durations as returned by `contentDetails.duration` (`PT1H2M3S`, `P1DT2H`,
//! `P0D` for upcoming streams).

/// Seconds, or `None` if the string is not a duration we understand.
pub fn parse_iso8601(s: &str) -> Option<u32> {
    let rest = s.strip_prefix('P')?;
    let (date, time) = match rest.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (rest, None),
    };
    let mut total: u64 = 0;
    total += sum_units(date, &[('W', 7 * 86_400), ('D', 86_400)])?;
    if let Some(t) = time {
        if t.is_empty() {
            return None;
        }
        total += sum_units(t, &[('H', 3600), ('M', 60), ('S', 1)])?;
    }
    u32::try_from(total).ok()
}

fn sum_units(s: &str, units: &[(char, u64)]) -> Option<u64> {
    let mut total = 0u64;
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let (_, secs) = units.iter().find(|(u, _)| *u == c)?;
        let n: u64 = num.parse().ok()?;
        total = total.checked_add(n.checked_mul(*secs)?)?;
        num.clear();
    }
    num.is_empty().then_some(total)
}

#[cfg(test)]
mod tests {
    use super::parse_iso8601 as p;

    #[test]
    fn parses_common_durations() {
        assert_eq!(p("PT15S"), Some(15));
        assert_eq!(p("PT1M3S"), Some(63));
        assert_eq!(p("PT2H"), Some(7200));
        assert_eq!(p("PT1H2M3S"), Some(3723));
        assert_eq!(p("P1DT2H"), Some(93_600));
        assert_eq!(p("P0D"), Some(0));
        assert_eq!(p("P1W"), Some(604_800));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(p(""), None);
        assert_eq!(p("PT"), None);
        assert_eq!(p("PT5"), None);
        assert_eq!(p("1M"), None);
        assert_eq!(p("PT5X"), None);
    }
}
