//! Repeated questions (step 4b): groups similar questions so the creator can answer
//! them once (or turn them into a video idea).
//!
//! Text → [`super::classify::normalize`] → content words (stop words and `?` removed,
//! a trailing plural `s` dropped) → character 3-shingles of the joined words → Jaccard
//! similarity. Grouping is single-link against each group's members, with threshold
//! [`THRESHOLD`]. The volume of one creator (at most [`MAX_QUESTIONS`] recent open
//! questions) makes the exact O(n²) comparison cheap, so no MinHash/LSH is needed.
//!
//! Cluster IDs are kept stable across rebuilds (a group reuses the ID most of its
//! members already had), so a cluster sent to the idea board stays linked.

use super::classify::normalize;
use crate::planning::{self, IdeaInput};
use crate::{AppError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use specta::Type;
use std::collections::{HashMap, HashSet};

pub const THRESHOLD: f64 = 0.4;
const MAX_QUESTIONS: usize = 2000;

const STOP: &[&str] = &[
    "que", "como", "cual", "cuando", "donde", "quien", "por", "para", "porque", "el", "la", "los",
    "las", "lo", "un", "una", "unos", "unas", "de", "del", "al", "a", "en", "y", "o", "es", "ese",
    "esa", "eso", "este", "esta", "esto", "se", "me", "te", "tu", "mi", "su", "usas", "usaste",
    "tienes", "hay", "son", "con", "sin", "hola", "oye", "bro", "pls", "porfa", "the", "a", "an",
    "is", "are", "what", "which", "how", "do", "you", "your", "that", "this", "it", "of", "to",
    "in", "on", "for", "and", "or", "use", "using", "hi", "hey", "please",
];

/// Content words of a question.
pub fn tokens(text: &str) -> Vec<String> {
    normalize(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1 && !STOP.contains(w))
        .map(|w| {
            if w.len() > 3 && w.ends_with('s') {
                w[..w.len() - 1].to_owned()
            } else {
                w.to_owned()
            }
        })
        .collect()
}

pub fn shingles(text: &str) -> HashSet<String> {
    let joined = format!(" {} ", tokens(text).join(" "));
    let chars: Vec<char> = joined.chars().collect();
    if chars.len() < 5 {
        return HashSet::new();
    }
    chars.windows(3).map(|w| w.iter().collect()).collect()
}

/// |A ∩ B| / |A ∪ B| (0 when either is empty).
pub fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    let inter = a.intersection(b).count() as f64;
    let union = (a.len() + b.len()) as f64 - inter;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Groups of indices into `texts` with at least two members.
pub fn group(texts: &[String]) -> Vec<Vec<usize>> {
    let sets: Vec<HashSet<String>> = texts.iter().map(|t| shingles(t)).collect();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    'next: for i in 0..texts.len() {
        if sets[i].is_empty() {
            continue;
        }
        for g in groups.iter_mut() {
            if g.iter().any(|&j| jaccard(&sets[i], &sets[j]) >= THRESHOLD) {
                g.push(i);
                continue 'next;
            }
        }
        groups.push(vec![i]);
    }
    groups.retain(|g| g.len() >= 2);
    groups
}

/// Rebuilds the clusters of open questions (pending or replied, not archived).
pub fn rebuild(conn: &Connection, now: &str) -> Result<()> {
    let rows: Vec<(String, String, Option<String>, i64)> = {
        let mut st = conn.prepare(
            "SELECT id, text, cluster_id, COALESCE(like_count, 0) FROM comments
             WHERE category = 'question' AND status <> 'archived' AND parent_id IS NULL
               AND is_own = 0
             ORDER BY published_at DESC LIMIT ?1",
        )?;
        let r = st.query_map(params![MAX_QUESTIONS as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
        r.collect::<std::result::Result<_, _>>()?
    };
    let texts: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
    let groups = group(&texts);

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE comments SET cluster_id = NULL WHERE cluster_id IS NOT NULL",
        [],
    )?;
    let mut used: HashSet<String> = HashSet::new();
    for g in &groups {
        // Reuse the ID most members had, if still free.
        let mut votes: HashMap<&str, usize> = HashMap::new();
        for &i in g {
            if let Some(c) = rows[i].2.as_deref() {
                *votes.entry(c).or_default() += 1;
            }
        }
        let mut candidates: Vec<(&str, usize)> = votes.into_iter().collect();
        candidates.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let reuse = candidates
            .into_iter()
            .map(|(id, _)| id.to_owned())
            .find(|id| !used.contains(id));
        // Label: the most liked question, then the shortest.
        let label_idx = *g
            .iter()
            .max_by(|&&a, &&b| {
                rows[a]
                    .3
                    .cmp(&rows[b].3)
                    .then(rows[b].1.chars().count().cmp(&rows[a].1.chars().count()))
            })
            .expect("non-empty group");
        let label: String = rows[label_idx].1.trim().chars().take(140).collect();
        let id = match reuse {
            Some(id) => {
                tx.execute(
                    "UPDATE question_clusters SET label = ?2, size = ?3, updated_at = ?4
                     WHERE id = ?1",
                    params![id, label, g.len() as i64, now],
                )?;
                id
            }
            None => {
                let id = uuid::Uuid::now_v7().to_string();
                tx.execute(
                    "INSERT INTO question_clusters (id, label, size, updated_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, label, g.len() as i64, now],
                )?;
                id
            }
        };
        for &i in g {
            tx.execute(
                "UPDATE comments SET cluster_id = ?2 WHERE id = ?1",
                params![rows[i].0, id],
            )?;
        }
        used.insert(id);
    }
    // Clusters that disappeared: forget them unless an idea came from them.
    let stale: Vec<String> = {
        let mut st = tx.prepare("SELECT id, idea_id FROM question_clusters")?;
        let r = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        r.collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|(id, _)| !used.contains(id))
            .map(|(id, idea)| {
                if idea.is_some() {
                    format!("keep:{id}")
                } else {
                    id
                }
            })
            .collect()
    };
    for s in stale {
        match s.strip_prefix("keep:") {
            Some(id) => {
                tx.execute(
                    "UPDATE question_clusters SET size = 0, updated_at = ?2 WHERE id = ?1",
                    params![id, now],
                )?;
            }
            None => {
                tx.execute("DELETE FROM question_clusters WHERE id = ?1", params![s])?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionCluster {
    pub id: String,
    pub label: String,
    pub size: u32,
    pub idea_id: Option<String>,
    /// A few of the questions, most liked first.
    pub samples: Vec<String>,
}

/// Current clusters (2+ questions), biggest first.
pub fn list(conn: &Connection) -> Result<Vec<QuestionCluster>> {
    let mut st = conn.prepare(
        "SELECT id, label, size, idea_id FROM question_clusters WHERE size >= 2
         ORDER BY size DESC, updated_at DESC LIMIT 50",
    )?;
    let mut out: Vec<QuestionCluster> = st
        .query_map([], |r| {
            Ok(QuestionCluster {
                id: r.get(0)?,
                label: r.get(1)?,
                size: r.get(2)?,
                idea_id: r.get(3)?,
                samples: Vec::new(),
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut s = conn.prepare(
        "SELECT text FROM comments WHERE cluster_id = ?1
         ORDER BY COALESCE(like_count, 0) DESC LIMIT 5",
    )?;
    for c in &mut out {
        c.samples = s
            .query_map(params![c.id], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
    }
    Ok(out)
}

/// "Send to the idea board": a new idea with the questions in its notes, linked to the
/// cluster. Sending the same cluster again returns the existing idea.
pub fn to_idea(conn: &Connection, cluster_id: &str, now: &str) -> Result<String> {
    let (label, idea): (String, Option<String>) = conn
        .query_row(
            "SELECT label, idea_id FROM question_clusters WHERE id = ?1",
            params![cluster_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or(AppError::NotFound)?;
    if let Some(id) = idea {
        if planning::get_idea(conn, &id).is_ok() {
            return Ok(id);
        }
    }
    let questions: Vec<String> = {
        let mut st = conn.prepare(
            "SELECT text FROM comments WHERE cluster_id = ?1
             ORDER BY COALESCE(like_count, 0) DESC LIMIT 15",
        )?;
        let r = st.query_map(params![cluster_id], |r| r.get(0))?;
        r.collect::<std::result::Result<_, _>>()?
    };
    let notes = questions
        .iter()
        .map(|q| format!("- {}", q.replace('\n', " ")))
        .collect::<Vec<_>>()
        .join("\n");
    let title: String = label.chars().take(200).collect();
    let created = planning::create_idea(
        conn,
        "idea",
        &IdeaInput {
            title,
            notes: Some(notes),
            target_platforms: Vec::new(),
            due_at: None,
        },
        now,
    )?;
    conn.execute(
        "UPDATE ideas SET source_cluster_id = ?2 WHERE id = ?1",
        params![created.id, cluster_id],
    )?;
    conn.execute(
        "UPDATE question_clusters SET idea_id = ?2 WHERE id = ?1",
        params![cluster_id, created.id],
    )?;
    Ok(created.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similar_questions_group_together() {
        let texts: Vec<String> = [
            "¿Qué mod usas para las sombras?",
            "que mod de sombras es ese??",
            "¿Cuándo sale la segunda parte?",
            "cuando sale la 2 parte??",
            "What shader pack is that?",
            "¿Cuál es tu configuración del ratón?",
            "qué mods de sombra usas",
        ]
        .map(String::from)
        .to_vec();
        let g = group(&texts);
        assert!(g.contains(&vec![0, 1, 6]), "{g:?}");
        assert!(g.contains(&vec![2, 3]), "{g:?}");
        assert_eq!(g.len(), 2, "singletons are dropped: {g:?}");
    }

    #[test]
    fn tokens_drop_stop_words_and_plurals() {
        assert_eq!(
            tokens("¿Qué mods usas para las sombras?"),
            vec!["mod", "sombra"]
        );
        assert!(shingles("¿qué?").is_empty());
        let a = shingles("mod sombras");
        assert_eq!(jaccard(&a, &a), 1.0);
        assert_eq!(jaccard(&a, &HashSet::new()), 0.0);
    }

    fn insert(conn: &Connection, id: &str, text: &str, likes: i64) {
        conn.execute(
            "INSERT INTO comments (id, account_id, external_id, text, category, like_count,
                                   published_at, fetched_at)
             VALUES (?1, 'acc', ?1, ?2, 'question', ?3, '2026-11-20T00:00:00Z', 't')",
            params![id, text, likes],
        )
        .unwrap();
    }

    #[test]
    fn rebuild_keeps_ids_and_sends_to_ideas() {
        let (db, ..) = super::super::tests::env();
        let conn = db.lock().unwrap();
        insert(&conn, "q1", "¿Qué mod usas para las sombras?", 1);
        insert(&conn, "q2", "que mod de sombras es ese??", 9);
        insert(&conn, "q3", "¿Cuándo sale la segunda parte?", 0);
        rebuild(&conn, "t1").unwrap();
        let first = list(&conn).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].size, 2);
        assert_eq!(first[0].label, "que mod de sombras es ese??", "most liked");
        assert_eq!(first[0].samples[0], "que mod de sombras es ese??");

        let idea = to_idea(&conn, &first[0].id, "t2").unwrap();
        assert_eq!(
            to_idea(&conn, &first[0].id, "t3").unwrap(),
            idea,
            "idempotent"
        );
        let i = planning::get_idea(&conn, &idea).unwrap();
        assert!(i
            .notes
            .unwrap()
            .contains("- ¿Qué mod usas para las sombras?"));

        insert(&conn, "q4", "qué mods de sombra usas", 0);
        rebuild(&conn, "t4").unwrap();
        let second = list(&conn).unwrap();
        assert_eq!(second[0].id, first[0].id, "stable ID");
        assert_eq!(second[0].size, 3);
        assert_eq!(second[0].idea_id.as_deref(), Some(idea.as_str()));

        // The questions get archived: the cluster linked to an idea is kept (size 0).
        conn.execute("UPDATE comments SET status = 'archived'", [])
            .unwrap();
        rebuild(&conn, "t5").unwrap();
        assert!(list(&conn).unwrap().is_empty());
        let size: i64 = conn
            .query_row(
                "SELECT size FROM question_clusters WHERE id = ?1",
                [&first[0].id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(size, 0);
    }
}
