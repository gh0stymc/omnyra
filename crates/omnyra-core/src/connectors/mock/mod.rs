//! Mock connector with deterministic fake data, for development and tests.

use super::*;

const MOCK_CONTENTS: u32 = 10;

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

    async fn fetch_channel_snapshot(&self, ctx: &AccountCtx) -> Result<ChannelSnapshot> {
        Ok(ChannelSnapshot {
            profile: self.fetch_profile(ctx).await?,
            totals: vec![
                MetricValue::new("followers", 1250.0),
                MetricValue::new("views_total", 98_000.0),
                MetricValue::new("contents_total", MOCK_CONTENTS as f64),
            ],
            created_at: Some("2026-08-01T00:00:00Z".into()),
        })
    }

    async fn fetch_channel_metrics(
        &self,
        _ctx: &AccountCtx,
        range: DateRange,
    ) -> Result<Vec<MetricPoint>> {
        let days = days_between(&range.from, &range.to)?;
        let mut out = Vec::with_capacity(days.len() * 4);
        for (i, day) in days.into_iter().enumerate() {
            let i = i as f64;
            // deterministic wobble, no RNG
            let views = (500.0 + (i * 1.7).sin().abs() * 300.0).round();
            for (metric, value) in [
                ("views", views),
                ("watch_time_s", views * 95.0),
                ("subscribers_gained", 3.0 + (i % 5.0)),
                ("subscribers_lost", 1.0),
            ] {
                out.push(MetricPoint {
                    day: day.clone(),
                    metric: metric.into(),
                    value,
                });
            }
        }
        Ok(out)
    }

    async fn list_contents(
        &self,
        _ctx: &AccountCtx,
        page: Option<String>,
    ) -> Result<Page<ContentRef>> {
        let start: u32 = page.as_deref().and_then(|p| p.parse().ok()).unwrap_or(0);
        let end = start.saturating_add(5).min(MOCK_CONTENTS);
        // Newest first, like real platforms.
        let items = (start..end)
            .map(|n| ContentRef {
                external_id: format!("mock-video-{}", MOCK_CONTENTS - 1 - n),
            })
            .collect::<Vec<_>>();
        let next = (end < MOCK_CONTENTS).then(|| end.to_string());
        Ok(Page { items, next })
    }

    async fn fetch_content_details(
        &self,
        _ctx: &AccountCtx,
        ids: &[String],
    ) -> Result<Vec<ContentDetails>> {
        Ok(ids
            .iter()
            .filter_map(|id| {
                let n: u32 = id.strip_prefix("mock-video-")?.parse().ok()?;
                Some(ContentDetails {
                    content: Content {
                        external_id: id.clone(),
                        kind: kind::VIDEO.into(),
                        title: Some(format!("Mock video #{n}")),
                        description: None,
                        url: None,
                        thumbnail_url: None,
                        published_at: Some(format!("2026-10-{:02}T18:00:00Z", n + 1)),
                        duration_s: Some(if n.is_multiple_of(3) {
                            40
                        } else {
                            300 + n * 10
                        }),
                        visibility: Some("public".into()),
                    },
                    kind_final: false,
                    stats: vec![
                        MetricValue::new("views", 1000.0 + n as f64 * 150.0),
                        MetricValue::new("likes", 50.0 + n as f64),
                    ],
                })
            })
            .collect())
    }

    async fn fetch_early_views(
        &self,
        _ctx: &AccountCtx,
        items: &[(String, String)],
        _days: u32,
    ) -> Result<Vec<(String, f64)>> {
        Ok(items
            .iter()
            .filter_map(|(id, _)| {
                let n: u32 = id.strip_prefix("mock-video-")?.parse().ok()?;
                Some((id.clone(), 200.0 + f64::from(n * 37 % 11) * 40.0))
            })
            .collect())
    }

    /// Every third mock video is a Short.
    async fn classify(&self, _ctx: &AccountCtx, ids: &[String]) -> Vec<(String, String)> {
        ids.iter()
            .filter_map(|id| {
                let n: u32 = id.strip_prefix("mock-video-")?.parse().ok()?;
                let k = if n.is_multiple_of(3) {
                    kind::SHORT
                } else {
                    kind::VIDEO
                };
                Some((id.clone(), k.to_owned()))
            })
            .collect()
    }

    fn check_upload(&self, draft: &UploadDraft, _facts: &FileFacts) -> Vec<Issue> {
        if draft.title.trim().is_empty() {
            vec![Issue::error("title", "required")]
        } else {
            Vec::new()
        }
    }

    async fn upload_options(&self, _ctx: &AccountCtx, _lang: &str) -> Result<UploadOptions> {
        Ok(UploadOptions {
            categories: vec![NamedOption {
                id: "1".into(),
                title: "Mock".into(),
            }],
            playlists: Vec::new(),
        })
    }

    /// Pretends to send the file in four steps; nothing leaves the machine.
    async fn upload(
        &self,
        _ctx: &AccountCtx,
        req: &UploadRequest,
        resume: Option<String>,
        sink: &dyn UploadSink,
    ) -> Result<UploadOutcome> {
        if resume.is_none() {
            sink.session_started("mock-session").await?;
        }
        for step in 1..=4u64 {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            sink.progress(req.size_bytes * step / 4, req.size_bytes);
        }
        Ok(UploadOutcome {
            external_id: format!("mock-upload-{}", req.size_bytes),
            warnings: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> AccountCtx {
        AccountCtx::unmetered("a", "m1", http::ApiClient::unauthenticated())
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
        assert_eq!(pts.len(), 40); // 10 days x 4 metrics
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
