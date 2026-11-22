//! Per-network security checklist with links to each platform's own settings. The APIs
//! do not expose 2FA or recovery settings, so answers are manual (`source = 'manual'`).

use crate::{AppError, Result};
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;

pub const STATES: &[&str] = &["unknown", "ok", "warning", "not_applicable"];

/// `(item, settings URL)` per platform.
pub fn items(platform: &str) -> &'static [(&'static str, &'static str)] {
    match platform {
        "youtube" => &[
            (
                "2fa",
                "https://myaccount.google.com/signinoptions/two-step-verification",
            ),
            (
                "passkeys",
                "https://myaccount.google.com/signinoptions/passkeys",
            ),
            ("recovery", "https://myaccount.google.com/recovery"),
            (
                "third_party_apps",
                "https://myaccount.google.com/connections",
            ),
            ("devices", "https://myaccount.google.com/device-activity"),
            (
                "channel_permissions",
                "https://studio.youtube.com/channel/UC/editing/permissions",
            ),
            (
                "security_checkup",
                "https://myaccount.google.com/security-checkup",
            ),
        ],
        "twitch" => &[
            ("2fa", "https://www.twitch.tv/settings/security"),
            ("recovery", "https://www.twitch.tv/settings/security"),
            (
                "third_party_apps",
                "https://www.twitch.tv/settings/connections",
            ),
            (
                "editors_mods",
                "https://dashboard.twitch.tv/community/roles",
            ),
            ("stream_key", "https://dashboard.twitch.tv/settings/stream"),
        ],
        "instagram" => &[
            (
                "2fa",
                "https://accountscenter.instagram.com/password_and_security/two_factor/",
            ),
            (
                "recovery",
                "https://accountscenter.instagram.com/personal_info/",
            ),
            (
                "third_party_apps",
                "https://www.instagram.com/accounts/manage_access/",
            ),
            (
                "login_alerts",
                "https://accountscenter.instagram.com/password_and_security/login_alerts/",
            ),
            (
                "devices",
                "https://accountscenter.instagram.com/password_and_security/login_activity/",
            ),
        ],
        "tiktok" => &[
            ("2fa", "https://www.tiktok.com/setting/security"),
            ("recovery", "https://www.tiktok.com/setting/account"),
            (
                "third_party_apps",
                "https://www.tiktok.com/setting/security",
            ),
            ("devices", "https://www.tiktok.com/setting/security"),
        ],
        _ => &[("2fa", ""), ("recovery", "")],
    }
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItem {
    pub item: String,
    pub url: String,
    pub state: String,
    pub checked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountChecklist {
    pub account_id: String,
    pub platform: String,
    pub display_name: String,
    pub items: Vec<ChecklistItem>,
    /// Share of applicable items marked `ok` (0..1).
    pub score: f64,
}

pub fn list(conn: &Connection) -> Result<Vec<AccountChecklist>> {
    let accounts: Vec<(String, String, String, String)> = {
        let mut st = conn.prepare(
            "SELECT id, platform, display_name, external_id FROM accounts
             WHERE status <> 'disconnected' AND platform <> 'mock' ORDER BY platform, display_name",
        )?;
        let r = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        r.collect::<std::result::Result<_, _>>()?
    };
    let mut get = conn.prepare(
        "SELECT state, checked_at FROM security_checklist WHERE account_id = ?1 AND item = ?2",
    )?;
    let mut out = Vec::new();
    for (id, platform, name, ext) in accounts {
        let mut list = Vec::new();
        for (item, url) in items(&platform) {
            let (state, checked_at): (String, Option<String>) = get
                .query_row(params![id, item], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap_or_else(|_| ("unknown".into(), None));
            list.push(ChecklistItem {
                item: (*item).into(),
                // The channel permissions page needs the channel ID.
                url: url.replace("/channel/UC/", &format!("/channel/{ext}/")),
                state,
                checked_at,
            });
        }
        let applicable = list.iter().filter(|i| i.state != "not_applicable").count();
        let ok = list.iter().filter(|i| i.state == "ok").count();
        out.push(AccountChecklist {
            score: if applicable == 0 {
                1.0
            } else {
                ok as f64 / applicable as f64
            },
            account_id: id,
            platform,
            display_name: name,
            items: list,
        });
    }
    Ok(out)
}

pub fn set(conn: &Connection, account_id: &str, item: &str, state: &str, now: &str) -> Result<()> {
    if !STATES.contains(&state) {
        return Err(AppError::Validation {
            field: "state".into(),
        });
    }
    let platform: String = conn
        .query_row(
            "SELECT platform FROM accounts WHERE id = ?1",
            params![account_id],
            |r| r.get(0),
        )
        .map_err(|_| AppError::NotFound)?;
    if !items(&platform).iter().any(|(i, _)| *i == item) {
        return Err(AppError::Validation {
            field: "item".into(),
        });
    }
    conn.execute(
        "INSERT INTO security_checklist (account_id, item, state, source, checked_at)
         VALUES (?1, ?2, ?3, 'manual', ?4)
         ON CONFLICT (account_id, item) DO UPDATE SET state = excluded.state,
                                                       checked_at = excluded.checked_at",
        params![account_id, item, state, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checklist_per_account_with_score() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (id, platform, external_id, display_name, connected_at)
               VALUES ('y', 'youtube', 'UCabc', 'gh0sty', 't'), ('m', 'mock', 'x', 'M', 't');",
        )
        .unwrap();
        set(&conn, "y", "2fa", "ok", "t1").unwrap();
        set(&conn, "y", "passkeys", "not_applicable", "t1").unwrap();
        assert!(set(&conn, "y", "nope", "ok", "t").is_err());
        assert!(set(&conn, "y", "2fa", "great", "t").is_err());
        let l = list(&conn).unwrap();
        assert_eq!(l.len(), 1, "mock accounts are skipped");
        let y = &l[0];
        assert!((y.score - 1.0 / 6.0).abs() < 1e-9);
        let perms = y
            .items
            .iter()
            .find(|i| i.item == "channel_permissions")
            .unwrap();
        assert!(perms.url.contains("/channel/UCabc/"));
        assert_eq!(
            y.items.iter().find(|i| i.item == "recovery").unwrap().state,
            "unknown"
        );
    }
}
