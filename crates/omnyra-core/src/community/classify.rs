//! Rule-based comment classification (step 4b): `spam`, `question`, `negative`,
//! `positive` or `neutral`, in that order of precedence, for Spanish and English.
//!
//! The rules sit behind [`Classifier`] so a model can replace them later. Comments the
//! user re-labelled by hand (`category_source = 'manual'`) are never overwritten.

use crate::{AppError, Result};
use rusqlite::{params, Connection};

pub const CATEGORIES: &[&str] = &["question", "spam", "negative", "positive", "neutral"];

pub trait Classifier {
    fn classify(&self, text: &str) -> &'static str;
}

/// Lowercase, accents removed, everything that is not a letter, digit, `?`, `#`, `@`,
/// `.`, `/` or `:` turned into a space; runs of the same letter (3+) shortened to two.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last: Option<char> = None;
    let mut run = 0;
    for ch in text.chars().flat_map(char::to_lowercase) {
        let c = match ch {
            'á' | 'à' | 'â' | 'ä' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            '¿' => '?',
            c if c.is_alphanumeric() || "?#@./:".contains(c) => c,
            _ => ' ',
        };
        if Some(c) == last && c.is_alphabetic() {
            run += 1;
            if run >= 2 {
                continue;
            }
        } else {
            run = 0;
        }
        last = Some(c);
        out.push(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

const QUESTION_STARTS: &[&str] = &[
    "que ",
    "como ",
    "cuando ",
    "donde ",
    "por que ",
    "porque ",
    "cual ",
    "cuales ",
    "quien ",
    "cuanto ",
    "cuantos ",
    "se puede ",
    "puedes ",
    "podrias ",
    "sabes ",
    "alguien sabe ",
    "hay ",
    "tienes ",
    "vas a ",
    "what ",
    "how ",
    "when ",
    "where ",
    "why ",
    "which ",
    "who ",
    "can you ",
    "could you ",
    "do you ",
    "does ",
    "did you ",
    "is it ",
    "is there ",
    "are you ",
    "will you ",
    "any ",
];

const SPAM: &[&str] = &[
    "gana dinero",
    "dinero facil",
    "desde casa",
    "escribeme",
    "escribeme por",
    "telegram",
    "whatsapp",
    "wa.me",
    "t.me",
    "sub4sub",
    "suscribete a mi canal",
    "mira mi canal",
    "visita mi canal",
    "pasate por mi canal",
    "check my channel",
    "check out my channel",
    "subscribe to my channel",
    "free robux",
    "robux gratis",
    "free vbucks",
    "pavos gratis",
    "click here",
    "haz click",
    "link in bio",
    "dm me",
    "earn money",
    "make money",
    "invest",
    "inversion",
    "bitcoin",
    "crypto",
    "forex",
    "onlyfans",
    "giveaway winner",
    "has ganado",
    "you won",
    "you have won",
    "ganador del sorteo",
    "claim your",
    "reclama tu premio",
];

const NEGATIVE: &[&str] = &[
    "basura",
    "aburrido",
    "aburre",
    "malo",
    "malisimo",
    "odio",
    "horrible",
    "peor",
    "cringe",
    "asco",
    "fake",
    "clickbait",
    "patetico",
    "penoso",
    "no me gusta",
    "me desuscribo",
    "dislike",
    "hate",
    "worst",
    "boring",
    "trash",
    "sucks",
    "terrible",
    "awful",
    "garbage",
    "unsubscribed",
    "unsubscribing",
    "waste of time",
    "perdida de tiempo",
    "decepcion",
    "disappointed",
    "mierda",
];

const POSITIVE: &[&str] = &[
    "encanta",
    "encantan",
    "genial",
    "increible",
    "buenisimo",
    "me gusta",
    "me gustan",
    "gracias",
    "crack",
    "top",
    "el mejor",
    "la mejor",
    "lo mejor",
    "mejor canal",
    "sigue asi",
    "bravo",
    "brutal",
    "epico",
    "love",
    "great",
    "awesome",
    "amazing",
    "nice",
    "best",
    "thanks",
    "thank you",
    "cool",
    "keep it up",
    "goat",
    "fire",
    "masterpiece",
    "obra maestra",
    "felicidades",
    "congrats",
];

const POSITIVE_EMOJI: &[char] = &['❤', '😍', '🔥', '👏', '💯', '🥰', '😂', '🤣', '💜', '👍'];
const NEGATIVE_EMOJI: &[char] = &['👎', '🤮', '💩', '😡'];

fn contains_term(text: &str, term: &str) -> bool {
    // Whole words for single tokens ("top" must not match "stop").
    if term.contains(' ') || term.contains('.') {
        return text.contains(term);
    }
    text.split(|c: char| !c.is_alphanumeric())
        .any(|w| w == term)
}

/// URL or bare domain (`something.tld`, path allowed) in `text`.
pub fn has_link(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("http://")
        || t.contains("https://")
        || t.contains("www.")
        || t.split_whitespace().any(|w| {
            let host = w
                .trim_matches(|c: char| !c.is_alphanumeric())
                .split('/')
                .next()
                .unwrap_or("");
            host.rsplit_once('.').is_some_and(|(name, tld)| {
                !name.is_empty()
                    && name.chars().any(|c| c.is_alphabetic())
                    && (2..=6).contains(&tld.len())
                    && tld.chars().all(|c| c.is_ascii_alphabetic())
            })
        })
}

pub struct Rules;

impl Classifier for Rules {
    fn classify(&self, text: &str) -> &'static str {
        let norm = normalize(text);
        let spam_hits = SPAM.iter().filter(|t| contains_term(&norm, t)).count();
        if spam_hits >= 2 || (spam_hits == 1 && has_link(text)) {
            return "spam";
        }
        let padded = format!("{norm} ");
        if norm.contains('?') || QUESTION_STARTS.iter().any(|q| padded.starts_with(q)) {
            return "question";
        }
        let neg = NEGATIVE.iter().filter(|t| contains_term(&norm, t)).count()
            + text.chars().filter(|c| NEGATIVE_EMOJI.contains(c)).count();
        let pos = POSITIVE.iter().filter(|t| contains_term(&norm, t)).count()
            + text.chars().filter(|c| POSITIVE_EMOJI.contains(c)).count();
        if neg > pos {
            "negative"
        } else if pos > 0 {
            "positive"
        } else {
            "neutral"
        }
    }
}

/// Classifies `ids` with the rules (manual labels are kept).
pub fn apply(conn: &Connection, ids: &[String]) -> Result<()> {
    let mut get = conn.prepare(
        "SELECT text FROM comments WHERE id = ?1 AND COALESCE(category_source, '') <> 'manual'",
    )?;
    let mut set =
        conn.prepare("UPDATE comments SET category = ?2, category_source = 'rules' WHERE id = ?1")?;
    for id in ids {
        let text: Option<String> =
            get.query_row(params![id], |r| r.get(0))
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })?;
        if let Some(t) = text {
            set.execute(params![id, Rules.classify(&t)])?;
        }
    }
    Ok(())
}

/// Re-runs the rules on every comment not labelled by hand (after a rules update).
pub fn reclassify_all(conn: &Connection) -> Result<u32> {
    let ids: Vec<String> = {
        let mut st = conn.prepare(
            "SELECT id FROM comments WHERE is_own = 0 AND COALESCE(category_source, '') <> 'manual'",
        )?;
        let rows = st.query_map([], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    apply(conn, &ids)?;
    Ok(ids.len() as u32)
}

/// Manual label; it sticks across syncs.
pub fn set_manual(conn: &Connection, id: &str, category: &str) -> Result<()> {
    if !CATEGORIES.contains(&category) {
        return Err(AppError::Validation {
            field: "category".into(),
        });
    }
    if conn.execute(
        "UPDATE comments SET category = ?2, category_source = 'manual' WHERE id = ?1",
        params![id, category],
    )? == 0
    {
        return Err(AppError::NotFound);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(t: &str) -> &'static str {
        Rules.classify(t)
    }

    #[test]
    fn normalizes_accents_case_and_stretching() {
        assert_eq!(normalize("¿QUÉ   Móóóod?!"), "?que mood?");
        assert_eq!(normalize("Holaaaaa!!! 😀"), "holaa");
        assert_eq!(normalize("Niño-Pingüino"), "nino pinguino");
    }

    #[test]
    fn questions_in_spanish_and_english() {
        for q in [
            "¿Qué mod usas?",
            "que mod es ese",
            "Cuándo sale la segunda parte",
            "como hiciste eso",
            "What shader pack is that",
            "how do you do the intro",
            "can you make a tutorial",
            "mod??",
            "Alguien sabe la canción",
        ] {
            assert_eq!(c(q), "question", "{q}");
        }
        // "que" in the middle is not a question.
        assert_eq!(c("lo mejor que he visto"), "positive");
    }

    #[test]
    fn spam_needs_two_signals_or_a_link() {
        assert_eq!(c("Gana dinero desde casa, escríbeme por Telegram"), "spam");
        assert_eq!(c("Free robux at robl0x-gift.com click now"), "spam");
        assert_eq!(c("check my channel www.youtube.com/@x"), "spam");
        // One weak signal alone is not spam.
        assert_eq!(c("¿Tienes telegram?"), "question");
        assert_eq!(c("Me encanta tu canal, suscríbete a mi canal"), "positive");
    }

    #[test]
    fn sentiment_with_words_and_emoji() {
        assert_eq!(c("Esto es una basura, qué aburrido"), "negative");
        assert_eq!(c("boring and clickbait"), "negative");
        assert_eq!(c("👎👎"), "negative");
        assert_eq!(c("¡Me encanta este vídeo, sigue así!"), "positive");
        assert_eq!(c("🔥🔥🔥"), "positive");
        assert_eq!(c("great video"), "positive");
        assert_eq!(c("primero"), "neutral");
        assert_eq!(c(""), "neutral");
        // Whole words: "stop" is not "top".
        assert_eq!(c("stop"), "neutral");
    }

    #[test]
    fn detects_links() {
        assert!(has_link("visita https://x.com"));
        assert!(has_link("entra en premios.xyz ya"));
        assert!(has_link("www.youtube.com/@x"));
        assert!(has_link("(t.me/canal)"));
        assert!(!has_link("1.5 veces mejor"));
        assert!(!has_link("fin."));
    }

    #[test]
    fn manual_labels_survive_reclassification() {
        let (db, ..) = super::super::tests::env();
        let conn = db.lock().unwrap();
        conn.execute(
            "INSERT INTO comments (id, account_id, external_id, text, fetched_at)
             VALUES ('c1', 'acc', 'e1', '¿qué tal?', 't'), ('c2', 'acc', 'e2', 'basura', 't')",
            [],
        )
        .unwrap();
        apply(&conn, &["c1".into(), "c2".into()]).unwrap();
        set_manual(&conn, "c1", "positive").unwrap();
        assert!(set_manual(&conn, "c1", "nope").is_err());
        assert_eq!(reclassify_all(&conn).unwrap(), 1);
        let cat = |id: &str| -> String {
            conn.query_row("SELECT category FROM comments WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
        };
        assert_eq!(cat("c1"), "positive");
        assert_eq!(cat("c2"), "negative");
    }
}
