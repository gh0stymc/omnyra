//! System notifications (step 2c). Messages are built here, in Spanish or English, because
//! they are rendered by the OS outside the webview where the frontend's i18n cannot reach.

use omnyra_core::db::repo::alerts::{kind, Alert};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

fn is_en(locale: &str) -> bool {
    locale == "en"
}

/// `1234` → `1.234` (es) / `1,234` (en). Alerts only carry whole numbers.
fn group(value: f64, en: bool) -> String {
    let n = value.round() as i64;
    let sep = if en { ',' } else { '.' };
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// `(title, body)` for an alert in the given locale.
pub fn alert_message(locale: &str, a: &Alert) -> (String, String) {
    let en = is_en(locale);
    let title = a.payload.content_title.clone().unwrap_or_default();
    match a.kind.as_str() {
        kind::MILESTONE => {
            let n = group(a.payload.value.unwrap_or_default(), en);
            if en {
                (
                    "Milestone reached".into(),
                    format!("You passed {n} followers"),
                )
            } else {
                (
                    "¡Hito alcanzado!".into(),
                    format!("Has superado los {n} seguidores"),
                )
            }
        }
        kind::OVERPERFORMING => {
            if en {
                (
                    "A video is taking off".into(),
                    format!("“{title}” is outperforming your usual numbers"),
                )
            } else {
                (
                    "Un vídeo está despuntando".into(),
                    format!("«{title}» rinde muy por encima de lo habitual"),
                )
            }
        }
        kind::UNDERPERFORMING => {
            if en {
                (
                    "A video is underperforming".into(),
                    format!("“{title}” is below your usual numbers"),
                )
            } else {
                (
                    "Un vídeo rinde por debajo".into(),
                    format!("«{title}» está por debajo de lo habitual"),
                )
            }
        }
        _ => {
            if en {
                ("Omnyra".into(), "You have a new alert".into())
            } else {
                ("Omnyra".into(), "Tienes una alerta nueva".into())
            }
        }
    }
}

/// Shows a system notification for an alert (best effort).
pub fn alert(app: &AppHandle, locale: &str, a: &Alert) {
    let (title, body) = alert_message(locale, a);
    show(app, &title, &body);
}

/// Shows a reminder notification (best effort).
pub fn reminder(app: &AppHandle, locale: &str, title: &str) {
    let heading = if is_en(locale) {
        "Reminder"
    } else {
        "Recordatorio"
    };
    show(app, heading, title);
}

fn show(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!(error = %e, "could not show notification");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands() {
        assert_eq!(group(1000.0, false), "1.000");
        assert_eq!(group(1000.0, true), "1,000");
        assert_eq!(group(50000.0, false), "50.000");
        assert_eq!(group(100.0, false), "100");
    }
}
