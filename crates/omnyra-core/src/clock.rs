use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Injectable clock so time-dependent logic is testable.
pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;

    fn now_iso(&self) -> String {
        self.now().format(&Rfc3339).expect("rfc3339 format")
    }

    /// UTC day as `YYYY-MM-DD`.
    fn today(&self) -> String {
        let d = self.now().date();
        format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day())
    }
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

pub struct FixedClock(pub OffsetDateTime);

impl Clock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn fixed_clock_formats_utc() {
        let c = FixedClock(datetime!(2026-11-16 20:43:00 UTC));
        assert_eq!(c.today(), "2026-11-16");
        assert_eq!(c.now_iso(), "2026-11-16T20:43:00Z");
    }
}
