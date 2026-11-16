//! Mock connector with deterministic fake data, for development and tests.

use super::*;

pub struct MockConnector {
    info: PlatformInfo,
}

impl MockConnector {
    pub fn new() -> Self {
        Self {
            info: PlatformInfo {
                id: "mock".into(),
                display_name: "Mock".into(),
                brand_color: "#8b5cf6".into(),
                capabilities: Capabilities {
                    analytics: true,
                    historic_analytics: true,
                    upload: UploadSupport::Direct,
                    resumable_upload: false,
                    schedule: ScheduleSupport::LocalOnly,
                    comments: CommentSupport::None,
                    live: false,
                    public_lookup: false,
                    limits: MediaLimits {
                        max_duration_s: Some(600),
                        max_size_mb: Some(1024),
                        max_title_len: Some(100),
                        max_description_len: Some(5000),
                        max_hashtags: Some(15),
                    },
                },
                auth: AuthKind::None,
                auth_configured: true,
            },
        }
    }
}

impl Default for MockConnector {
    fn default() -> Self {
        Self::new()
    }
}

/// Days between two `YYYY-MM-DD` strings, inclusive, as a vector of day strings.
fn days_between(from: &str, to: &str) -> Result<Vec<String>> {
    use time::{macros::format_description, Date, Duration};
    let fmt = format_description!("[year]-[month]-[day]");
    let parse = |s: &str, field: &str| {
        Date::parse(s, &fmt).map_err(|_| AppError::Validation {
            field: field.into(),
        })
    };
    let (mut d, end) = (parse(from, "from")?, parse(to, "to")?);
    let mut out = Vec::new();
    while d <= end && out.len() < 3660 {
        out.push(d.format(&fmt).expect("format"));
        d += Duration::days(1);
    }
    Ok(out)
}

#[async_trait]
impl Connector for MockConnector {
    fn info(&self) -> &PlatformInfo {
        &self.info
    }

    async fn fetch_profile(&self, ctx: &AccountCtx) -> Result<Profile> {
        Ok(Profile {
            external_id: ctx.external_id.clone(),
            handle: Some("@mock_creator".into()),
            display_name: "Mock Creator".into(),
            avatar_url: None,
        })
    }

    async fn fetch_channel_metrics(
        &self,
        _ctx: &AccountCtx,
        range: DateRange,
    ) -> Result<Vec<MetricPoint>> {
        let days = days_between(&range.from, &range.to)?;
        let mut out = Vec::with_capacity(days.len() * 2);
        for (i, day) in days.into_iter().enumerate() {
            let i = i as f64;
            out.push(MetricPoint {
                day: day.clone(),
                metric: "followers".into(),
                value: 1000.0 + i * 7.0,
            });
            // deterministic wobble, no RNG
            out.push(MetricPoint {
                day,
                metric: "views".into(),
                value: 500.0 + (i * 1.7).sin().abs() * 300.0,
            });
        }
        Ok(out)
    }

    async fn list_contents(
        &self,
        _ctx: &AccountCtx,
        page: Option<String>,
    ) -> Result<Page<Content>> {
        let start: u32 = page.as_deref().and_then(|p| p.parse().ok()).unwrap_or(0);
        let items = (start..start.saturating_add(5).min(10))
            .map(|n| Content {
                external_id: format!("mock-video-{n}"),
                kind: "video".into(),
                title: Some(format!("Mock video #{n}")),
                url: None,
                published_at: Some(format!("2026-10-{:02}T18:00:00Z", n + 1)),
                duration_s: Some(300 + n * 10),
            })
            .collect::<Vec<_>>();
        let next = if start + 5 < 10 {
            Some((start + 5).to_string())
        } else {
            None
        };
        Ok(Page { items, next })
    }

    async fn upload(&self, _ctx: &AccountCtx, req: UploadRequest) -> Result<UploadOutcome> {
        if req.title.trim().is_empty() {
            return Err(AppError::Validation {
                field: "title".into(),
            });
        }
        Ok(UploadOutcome {
            external_id: "mock-uploaded".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> AccountCtx {
        AccountCtx {
            account_id: "a".into(),
            external_id: "m1".into(),
            api: http::ApiClient::unauthenticated(),
        }
    }

    #[tokio::test]
    async fn metrics_cover_the_range() {
        let c = MockConnector::new();
        let pts = c
            .fetch_channel_metrics(
                &ctx(),
                DateRange {
                    from: "2026-11-01".into(),
                    to: "2026-11-10".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(pts.len(), 20); // 10 days x 2 metrics
    }

    #[tokio::test]
    async fn bad_range_is_a_validation_error() {
        let c = MockConnector::new();
        let err = c
            .fetch_channel_metrics(
                &ctx(),
                DateRange {
                    from: "nope".into(),
                    to: "2026-11-10".into(),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation { .. }));
    }

    #[tokio::test]
    async fn pagination_terminates() {
        let c = MockConnector::new();
        let p1 = c.list_contents(&ctx(), None).await.unwrap();
        assert_eq!(p1.items.len(), 5);
        let p2 = c.list_contents(&ctx(), p1.next).await.unwrap();
        assert_eq!(p2.items.len(), 5);
        assert!(p2.next.is_none());
    }

    #[tokio::test]
    async fn unsupported_capability_errors() {
        let c = MockConnector::new();
        let err = c.list_comments(&ctx(), "x", None).await.unwrap_err();
        assert!(matches!(err, AppError::NotSupported));
    }
}
