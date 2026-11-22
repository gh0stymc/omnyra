//! Sponsorships (step 6a): brand deals with contact, deliverables, dates, amount, payment
//! status and attached files (by reference), plus the media kit generator.

use crate::{dashboard, AppError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::path::Path;

pub const STATUSES: &[&str] = &[
    "prospect",
    "negotiating",
    "signed",
    "in_progress",
    "delivered",
    "cancelled",
];
pub const PAYMENT: &[&str] = &["pending", "invoiced", "paid"];

fn invalid(field: &str) -> AppError {
    AppError::Validation {
        field: field.into(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Deliverable {
    /// `None` on input = new.
    pub id: Option<String>,
    pub description: String,
    pub platform: Option<String>,
    /// `YYYY-MM-DD`.
    pub due_at: Option<String>,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SponsorFile {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sponsorship {
    pub id: String,
    pub brand: String,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub status: String,
    /// Cents (f64 for TypeScript).
    pub amount_cents: Option<f64>,
    pub currency: Option<String>,
    pub payment_status: String,
    pub payment_due: Option<String>,
    /// Payment pending/invoiced and `payment_due` already past.
    pub overdue: bool,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub notes: Option<String>,
    pub deliverables: Vec<Deliverable>,
    pub files: Vec<SponsorFile>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SponsorshipInput {
    pub brand: String,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub status: String,
    pub amount_cents: Option<f64>,
    pub currency: Option<String>,
    pub payment_status: String,
    pub payment_due: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub notes: Option<String>,
    pub deliverables: Vec<Deliverable>,
}

fn check_day(d: &Option<String>, field: &str) -> Result<()> {
    if let Some(d) = d {
        crate::sync::parse_day(d).map_err(|_| invalid(field))?;
        if d.len() != 10 {
            return Err(invalid(field));
        }
    }
    Ok(())
}

fn check(i: &SponsorshipInput) -> Result<()> {
    if i.brand.trim().is_empty() || i.brand.chars().count() > 120 {
        return Err(invalid("brand"));
    }
    if !STATUSES.contains(&i.status.as_str()) {
        return Err(invalid("status"));
    }
    if !PAYMENT.contains(&i.payment_status.as_str()) {
        return Err(invalid("paymentStatus"));
    }
    if let Some(a) = i.amount_cents {
        if !(0.0..=1e13).contains(&a) || a.fract() != 0.0 {
            return Err(invalid("amount"));
        }
    }
    if let Some(c) = &i.currency {
        if c.len() != 3 || !c.chars().all(|x| x.is_ascii_uppercase()) {
            return Err(invalid("currency"));
        }
    }
    if let Some(e) = &i.contact_email {
        let e = e.trim();
        if !e.is_empty() && (!e.contains('@') || e.contains(char::is_whitespace)) {
            return Err(invalid("contactEmail"));
        }
    }
    for (d, f) in [
        (&i.payment_due, "paymentDue"),
        (&i.start_date, "startDate"),
        (&i.end_date, "endDate"),
    ] {
        check_day(d, f)?;
    }
    if let (Some(s), Some(e)) = (&i.start_date, &i.end_date) {
        if e < s {
            return Err(invalid("endDate"));
        }
    }
    for d in &i.deliverables {
        if d.description.trim().is_empty() {
            return Err(invalid("deliverables"));
        }
        check_day(&d.due_at, "deliverables")?;
    }
    Ok(())
}

fn blank(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty())
}

fn write_deliverables(conn: &Connection, id: &str, items: &[Deliverable]) -> Result<()> {
    conn.execute(
        "DELETE FROM sponsorship_deliverables WHERE sponsorship_id = ?1",
        params![id],
    )?;
    for (pos, d) in items.iter().enumerate() {
        conn.execute(
            "INSERT INTO sponsorship_deliverables (id, sponsorship_id, description, platform,
                                                   due_at, done, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                d.id.clone()
                    .unwrap_or_else(|| uuid::Uuid::now_v7().to_string()),
                id,
                d.description.trim(),
                blank(&d.platform),
                d.due_at,
                d.done,
                pos as i64
            ],
        )?;
    }
    Ok(())
}

pub fn create(conn: &Connection, i: &SponsorshipInput, now: &str) -> Result<Sponsorship> {
    check(i)?;
    let id = uuid::Uuid::now_v7().to_string();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO sponsorships (id, brand, contact_name, contact_email, status, amount_cents,
                                   currency, payment_status, payment_due, start_date, end_date,
                                   notes, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
        params![
            id,
            i.brand.trim(),
            blank(&i.contact_name),
            blank(&i.contact_email),
            i.status,
            i.amount_cents.map(|a| a as i64),
            i.currency,
            i.payment_status,
            i.payment_due,
            i.start_date,
            i.end_date,
            blank(&i.notes),
            now
        ],
    )?;
    write_deliverables(&tx, &id, &i.deliverables)?;
    tx.commit()?;
    get(conn, &id, now)
}

pub fn update(conn: &Connection, id: &str, i: &SponsorshipInput, now: &str) -> Result<Sponsorship> {
    check(i)?;
    let tx = conn.unchecked_transaction()?;
    let n = tx.execute(
        "UPDATE sponsorships SET brand = ?2, contact_name = ?3, contact_email = ?4, status = ?5,
                amount_cents = ?6, currency = ?7, payment_status = ?8, payment_due = ?9,
                start_date = ?10, end_date = ?11, notes = ?12, updated_at = ?13
         WHERE id = ?1",
        params![
            id,
            i.brand.trim(),
            blank(&i.contact_name),
            blank(&i.contact_email),
            i.status,
            i.amount_cents.map(|a| a as i64),
            i.currency,
            i.payment_status,
            i.payment_due,
            i.start_date,
            i.end_date,
            blank(&i.notes),
            now
        ],
    )?;
    if n == 0 {
        return Err(AppError::NotFound);
    }
    write_deliverables(&tx, id, &i.deliverables)?;
    tx.commit()?;
    get(conn, id, now)
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    let files: Vec<String> = {
        let mut st =
            conn.prepare("SELECT file_id FROM sponsorship_files WHERE sponsorship_id = ?1")?;
        let r = st.query_map(params![id], |r| r.get(0))?;
        r.collect::<std::result::Result<_, _>>()?
    };
    if conn.execute("DELETE FROM sponsorships WHERE id = ?1", params![id])? == 0 {
        return Err(AppError::NotFound);
    }
    for f in files {
        // References only: the user's files stay on disk.
        conn.execute(
            "DELETE FROM files WHERE id = ?1 AND managed = 0
               AND NOT EXISTS (SELECT 1 FROM sponsorship_files WHERE file_id = ?1)",
            params![f],
        )?;
    }
    Ok(())
}

const COLS: &str = "id, brand, contact_name, contact_email, status, amount_cents, currency,
                    payment_status, payment_due, start_date, end_date, notes, updated_at";

fn row(r: &rusqlite::Row, today: &str) -> rusqlite::Result<Sponsorship> {
    let payment_status: String = r.get(7)?;
    let payment_due: Option<String> = r.get(8)?;
    let status: String = r.get(4)?;
    Ok(Sponsorship {
        id: r.get(0)?,
        brand: r.get(1)?,
        contact_name: r.get(2)?,
        contact_email: r.get(3)?,
        overdue: payment_status != "paid"
            && status != "cancelled"
            && payment_due.as_deref().is_some_and(|d| d < today),
        status,
        amount_cents: r.get::<_, Option<i64>>(5)?.map(|a| a as f64),
        currency: r.get(6)?,
        payment_status,
        payment_due,
        start_date: r.get(9)?,
        end_date: r.get(10)?,
        notes: r.get(11)?,
        updated_at: r.get(12)?,
        deliverables: Vec::new(),
        files: Vec::new(),
    })
}

fn fill(conn: &Connection, s: &mut Sponsorship) -> Result<()> {
    let mut st = conn.prepare(
        "SELECT id, description, platform, due_at, done FROM sponsorship_deliverables
         WHERE sponsorship_id = ?1 ORDER BY position",
    )?;
    s.deliverables = st
        .query_map(params![s.id], |r| {
            Ok(Deliverable {
                id: r.get(0)?,
                description: r.get(1)?,
                platform: r.get(2)?,
                due_at: r.get(3)?,
                done: r.get(4)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut st = conn.prepare(
        "SELECT f.id, COALESCE(f.original_name, f.path), f.path FROM sponsorship_files s
         JOIN files f ON f.id = s.file_id WHERE s.sponsorship_id = ?1 ORDER BY f.created_at",
    )?;
    s.files = st
        .query_map(params![s.id], |r| {
            Ok(SponsorFile {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str, now: &str) -> Result<Sponsorship> {
    let today = now.get(..10).unwrap_or(now).to_owned();
    let mut s = conn
        .query_row(
            &format!("SELECT {COLS} FROM sponsorships WHERE id = ?1"),
            params![id],
            |r| row(r, &today),
        )
        .optional()?
        .ok_or(AppError::NotFound)?;
    fill(conn, &mut s)?;
    Ok(s)
}

pub fn list(conn: &Connection, now: &str) -> Result<Vec<Sponsorship>> {
    let today = now.get(..10).unwrap_or(now).to_owned();
    let mut st = conn.prepare(&format!(
        "SELECT {COLS} FROM sponsorships ORDER BY COALESCE(start_date, created_at) DESC"
    ))?;
    let mut out: Vec<Sponsorship> = st
        .query_map([], |r| row(r, &today))?
        .collect::<std::result::Result<_, _>>()?;
    for s in &mut out {
        fill(conn, s)?;
    }
    Ok(out)
}

pub fn attach_file(conn: &Connection, id: &str, path: &str, now: &str) -> Result<Sponsorship> {
    let meta = std::fs::metadata(path).map_err(|_| AppError::FileUnavailable)?;
    if !meta.is_file() {
        return Err(AppError::FileUnavailable);
    }
    get(conn, id, now)?;
    let file_id = uuid::Uuid::now_v7().to_string();
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned());
    conn.execute(
        "INSERT INTO files (id, path, managed, original_name, size_bytes, created_at)
         VALUES (?1, ?2, 0, ?3, ?4, ?5)",
        params![file_id, path, name, meta.len() as i64, now],
    )?;
    conn.execute(
        "INSERT INTO sponsorship_files (sponsorship_id, file_id) VALUES (?1, ?2)",
        params![id, file_id],
    )?;
    get(conn, id, now)
}

pub fn detach_file(conn: &Connection, id: &str, file_id: &str, now: &str) -> Result<Sponsorship> {
    conn.execute(
        "DELETE FROM sponsorship_files WHERE sponsorship_id = ?1 AND file_id = ?2",
        params![id, file_id],
    )?;
    conn.execute(
        "DELETE FROM files WHERE id = ?1 AND managed = 0
           AND NOT EXISTS (SELECT 1 FROM sponsorship_files WHERE file_id = ?1)",
        params![file_id],
    )?;
    get(conn, id, now)
}

/// Only files attached to a sponsorship can be opened (no generic "open any path").
pub fn attached_path(conn: &Connection, file_id: &str) -> Result<String> {
    conn.query_row(
        "SELECT f.path FROM files f JOIN sponsorship_files s ON s.file_id = f.id
         WHERE f.id = ?1 LIMIT 1",
        params![file_id],
        |r| r.get(0),
    )
    .optional()?
    .ok_or(AppError::NotFound)
}

#[derive(Debug, Clone, Serialize, Type, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MoneyTotals {
    pub currency: String,
    pub signed_cents: f64,
    pub paid_cents: f64,
    pub pending_cents: f64,
    pub overdue_cents: f64,
}

/// Money per currency over non-cancelled deals that reached `signed` or later.
pub fn totals(list: &[Sponsorship]) -> Vec<MoneyTotals> {
    let mut by: BTreeMap<String, MoneyTotals> = BTreeMap::new();
    for s in list {
        if !matches!(s.status.as_str(), "signed" | "in_progress" | "delivered") {
            continue;
        }
        let (Some(a), Some(c)) = (s.amount_cents, s.currency.clone()) else {
            continue;
        };
        let t = by.entry(c.clone()).or_insert_with(|| MoneyTotals {
            currency: c,
            ..Default::default()
        });
        t.signed_cents += a;
        if s.payment_status == "paid" {
            t.paid_cents += a;
        } else {
            t.pending_cents += a;
            if s.overdue {
                t.overdue_cents += a;
            }
        }
    }
    by.into_values().collect()
}

// ---- media kit -------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaKitConfig {
    pub title: String,
    pub bio: String,
    pub contact_email: String,
    /// Accounts to include.
    pub account_ids: Vec<String>,
    /// Which figures: `followers`, `views30`, `avgViews`, `engagement`, `topContents`.
    pub metrics: Vec<String>,
    /// `#rrggbb`.
    pub accent: String,
    /// `es` | `en`.
    pub language: String,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn compact(n: f64, lang: &str) -> String {
    let (v, suffix) = if n >= 1e6 {
        (n / 1e6, "M")
    } else if n >= 1e3 {
        (n / 1e3, "K")
    } else {
        return format!("{}", n.round() as i64);
    };
    let s = if v >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    };
    let s = if lang == "es" { s.replace('.', ",") } else { s };
    format!("{s}{suffix}")
}

/// Self-contained HTML (inline CSS, no external resources) ready to save or print to PDF.
pub fn media_kit_html(conn: &Connection, cfg: &MediaKitConfig, now: &str) -> Result<String> {
    let accent = if cfg.accent.len() == 7
        && cfg.accent.starts_with('#')
        && cfg.accent[1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        cfg.accent.clone()
    } else {
        "#7c3aed".into()
    };
    let es = cfg.language != "en";
    let l = |es_s: &'static str, en_s: &'static str| if es { es_s } else { en_s };
    let lang = if es { "es" } else { "en" };
    let today = now.get(..10).unwrap_or(now);
    let from = crate::sync::fmt_day(crate::sync::parse_day(today)? - time::Duration::days(30));
    let summary = dashboard::summary(conn, Some(&from), today)?;
    let want = |m: &str| cfg.metrics.iter().any(|x| x == m);

    let mut cards = String::new();
    let mut total_followers = 0.0;
    for a in summary
        .accounts
        .iter()
        .filter(|a| cfg.account_ids.contains(&a.account_id))
    {
        let contents = crate::db::repo::contents::list_with_stats(conn, &a.account_id)?;
        let recent: Vec<_> = contents
            .iter()
            .filter(|c| {
                c.published_at
                    .as_deref()
                    .is_some_and(|p| p >= from.as_str())
            })
            .collect();
        let views30 = if a.views > 0.0 {
            a.views
        } else {
            recent.iter().filter_map(|c| c.views).sum()
        };
        let with_views: Vec<f64> = contents.iter().take(20).filter_map(|c| c.views).collect();
        let avg_views = (!with_views.is_empty())
            .then(|| with_views.iter().sum::<f64>() / with_views.len() as f64);
        let inter: f64 = contents
            .iter()
            .take(20)
            .map(|c| c.likes.unwrap_or(0.0) + c.comments.unwrap_or(0.0))
            .sum();
        let engagement = (with_views.iter().sum::<f64>() > 0.0)
            .then(|| inter / with_views.iter().sum::<f64>() * 100.0);
        total_followers += a.followers.unwrap_or(0.0);

        let mut stats = String::new();
        let mut stat = |label: &str, value: String| {
            stats.push_str(&format!(
                "<div class=\"stat\"><b>{}</b><span>{}</span></div>",
                esc(&value),
                esc(label)
            ));
        };
        if want("followers") {
            stat(
                l("seguidores", "followers"),
                a.followers
                    .map(|f| compact(f, lang))
                    .unwrap_or_else(|| "—".into()),
            );
        }
        if want("views30") {
            stat(
                l("vistas en 30 días", "views in 30 days"),
                compact(views30, lang),
            );
        }
        if want("avgViews") {
            stat(
                l("vistas medias por vídeo", "average views per video"),
                avg_views
                    .map(|v| compact(v, lang))
                    .unwrap_or_else(|| "—".into()),
            );
        }
        if want("engagement") {
            stat(
                l("interacción", "engagement"),
                engagement
                    .map(|e| {
                        let s = format!("{e:.1} %");
                        if es {
                            s.replace('.', ",")
                        } else {
                            s
                        }
                    })
                    .unwrap_or_else(|| "—".into()),
            );
        }
        let mut top = String::new();
        if want("topContents") {
            let mut best: Vec<_> = contents.iter().filter(|c| c.views.is_some()).collect();
            best.sort_by(|x, y| {
                y.views
                    .partial_cmp(&x.views)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for c in best.into_iter().take(3) {
                top.push_str(&format!(
                    "<li>{} <small>{} {}</small></li>",
                    esc(c.title.as_deref().unwrap_or("—")),
                    compact(c.views.unwrap_or(0.0), lang),
                    l("vistas", "views")
                ));
            }
            if !top.is_empty() {
                top = format!(
                    "<h3>{}</h3><ol>{top}</ol>",
                    l("Lo más visto", "Top content")
                );
            }
        }
        cards.push_str(&format!(
            "<section class=\"card\"><h2>{} <small>{}{}</small></h2><div class=\"stats\">{stats}</div>{top}</section>",
            esc(&a.display_name),
            esc(&a.platform),
            a.handle.as_deref().map(|h| format!(" · @{}", esc(h.trim_start_matches('@')))).unwrap_or_default(),
        ));
    }
    let contact = if cfg.contact_email.trim().is_empty() {
        String::new()
    } else {
        format!(
            "<p class=\"contact\">{}: <a href=\"mailto:{e}\">{e}</a></p>",
            l("Contacto", "Contact"),
            e = esc(cfg.contact_email.trim())
        )
    };
    Ok(format!(
        r#"<!doctype html>
<html lang="{lang}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>
:root{{--a:{accent}}}
*{{box-sizing:border-box}}body{{margin:0;font-family:system-ui,-apple-system,"Segoe UI",sans-serif;color:#111;background:#fff}}
header{{background:var(--a);color:#fff;padding:40px 48px}}header h1{{margin:0;font-size:36px}}header p{{margin:8px 0 0;max-width:640px;opacity:.95;white-space:pre-wrap}}
.total{{margin-top:16px;font-size:18px}}main{{padding:32px 48px;display:grid;gap:20px;grid-template-columns:repeat(auto-fit,minmax(300px,1fr))}}
.card{{border:1px solid #e5e7eb;border-radius:14px;padding:20px;break-inside:avoid}}.card h2{{margin:0 0 12px;font-size:20px}}.card h2 small{{color:#666;font-weight:400;font-size:13px}}
.stats{{display:grid;grid-template-columns:repeat(2,1fr);gap:12px}}.stat b{{display:block;font-size:26px;color:var(--a)}}.stat span{{font-size:12px;color:#555}}
h3{{font-size:14px;margin:16px 0 6px}}ol{{margin:0;padding-left:18px;font-size:13px}}ol small{{color:#666}}
footer{{padding:0 48px 32px;color:#555;font-size:13px}}.contact a{{color:var(--a)}}
@media print{{header{{-webkit-print-color-adjust:exact;print-color-adjust:exact}}}}
</style></head><body>
<header><h1>{title}</h1><p>{bio}</p><div class="total">{total} {total_label}</div></header>
<main>{cards}</main>
<footer>{contact}<p>{updated}: {today}</p></footer>
</body></html>"#,
        title = esc(cfg.title.trim()),
        bio = esc(cfg.bio.trim()),
        total = compact(total_followers, lang),
        total_label = l("seguidores en total", "total followers"),
        updated = l("Datos actualizados", "Data as of"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn input() -> SponsorshipInput {
        SponsorshipInput {
            brand: "  Marca X ".into(),
            contact_name: Some("Ana".into()),
            contact_email: Some("ana@marca.com".into()),
            status: "signed".into(),
            amount_cents: Some(50000.0),
            currency: Some("EUR".into()),
            payment_status: "invoiced".into(),
            payment_due: Some("2026-11-01".into()),
            start_date: Some("2026-10-01".into()),
            end_date: Some("2026-12-01".into()),
            notes: None,
            deliverables: vec![Deliverable {
                id: None,
                description: "Vídeo integrado 60 s".into(),
                platform: Some("youtube".into()),
                due_at: Some("2026-11-15".into()),
                done: false,
            }],
        }
    }

    #[test]
    fn crud_with_deliverables_overdue_and_totals() {
        let conn = open_in_memory().unwrap();
        let s = create(&conn, &input(), "2026-11-22T10:00:00Z").unwrap();
        assert_eq!(s.brand, "Marca X");
        assert!(s.overdue, "invoiced and due date passed");
        assert_eq!(s.deliverables.len(), 1);
        let mut i = input();
        i.payment_status = "paid".into();
        i.deliverables[0].done = true;
        i.deliverables.push(Deliverable {
            id: None,
            description: "Story".into(),
            platform: None,
            due_at: None,
            done: false,
        });
        let s2 = update(&conn, &s.id, &i, "2026-11-22T11:00:00Z").unwrap();
        assert!(!s2.overdue);
        assert_eq!(s2.deliverables.len(), 2);
        assert!(s2.deliverables[0].done);

        let mut other = input();
        other.brand = "B".into();
        other.amount_cents = Some(10000.0);
        create(&conn, &other, "2026-11-22T10:00:00Z").unwrap();
        let mut prospect = input();
        prospect.status = "prospect".into();
        create(&conn, &prospect, "2026-11-22T10:00:00Z").unwrap();
        let all = list(&conn, "2026-11-22T12:00:00Z").unwrap();
        assert_eq!(all.len(), 3);
        let t = totals(&all);
        assert_eq!(t.len(), 1);
        assert_eq!(
            (t[0].signed_cents, t[0].paid_cents, t[0].overdue_cents),
            (60000.0, 50000.0, 10000.0)
        );

        delete(&conn, &s.id).unwrap();
        assert_eq!(list(&conn, "t").unwrap().len(), 2);
    }

    #[test]
    fn validates_fields() {
        let conn = open_in_memory().unwrap();
        let bad = |f: fn(&mut SponsorshipInput)| {
            let mut i = input();
            f(&mut i);
            create(&conn, &i, "t").is_err()
        };
        assert!(bad(|i| i.brand = " ".into()));
        assert!(bad(|i| i.status = "x".into()));
        assert!(bad(|i| i.currency = Some("eur".into())));
        assert!(bad(|i| i.amount_cents = Some(1.5)));
        assert!(bad(|i| i.contact_email = Some("no-at".into())));
        assert!(bad(|i| i.end_date = Some("2026-01-01".into())));
        assert!(bad(|i| i.payment_due = Some("2026-13-40".into())));
    }

    #[test]
    fn files_are_references() {
        let conn = open_in_memory().unwrap();
        let s = create(&conn, &input(), "t").unwrap();
        let p = std::env::temp_dir().join(format!("omnyra-sp-{}.pdf", uuid::Uuid::now_v7()));
        std::fs::write(&p, b"%PDF").unwrap();
        let s = attach_file(&conn, &s.id, &p.to_string_lossy(), "t").unwrap();
        assert_eq!(s.files.len(), 1);
        assert_eq!(
            attached_path(&conn, &s.files[0].id).unwrap(),
            p.to_string_lossy()
        );
        let s = detach_file(&conn, &s.id, &s.files[0].id.clone(), "t").unwrap();
        assert!(s.files.is_empty());
        assert!(p.exists(), "the user's file is never deleted");
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn media_kit_is_escaped_and_selfcontained() {
        let conn = open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (id, platform, external_id, display_name, handle, connected_at)
               VALUES ('a', 'youtube', 'x', 'gh0sty <script>', 'gh0stymc', 't');
             INSERT INTO account_snapshots VALUES ('a', '2026-11-20', 'followers', 12345, 'current', 't');",
        )
        .unwrap();
        let html = media_kit_html(
            &conn,
            &MediaKitConfig {
                title: "Media kit".into(),
                bio: "Minecraft & más".into(),
                contact_email: "x@y.com".into(),
                account_ids: vec!["a".into()],
                metrics: vec!["followers".into(), "engagement".into()],
                accent: "javascript:alert(1)".into(),
                language: "es".into(),
            },
            "2026-11-22T10:00:00Z",
        )
        .unwrap();
        assert!(html.contains("gh0sty &lt;script&gt;"));
        assert!(html.contains("Minecraft &amp; más"));
        assert!(html.contains("12,3K"));
        assert!(html.contains("#7c3aed"), "invalid accent falls back");
        assert!(!html.contains("http://") && !html.contains("https://"));
        assert!(html.contains("mailto:x@y.com"));
    }

    #[test]
    fn compact_numbers() {
        assert_eq!(compact(999.0, "en"), "999");
        assert_eq!(compact(1500.0, "en"), "1.5K");
        assert_eq!(compact(2_340_000.0, "es"), "2,3M");
        assert_eq!(compact(250_000.0, "en"), "250K");
    }
}
