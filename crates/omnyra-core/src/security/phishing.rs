//! Phishing and scam detection in comments (step 6c).
//!
//! Explainable score in `0..=1` from independent signals, each with a reason code:
//!
//! | code | signal | weight |
//! |---|---|---|
//! | `lookalike_domain` | a domain that imitates a known brand (edit distance or homoglyphs) | 0.6 |
//! | `punycode` | an internationalised (`xn--`) domain | 0.4 |
//! | `obfuscated_link` | a link written to dodge filters (`hxxp`, `[.]`, `(dot)`, `t . me`) | 0.35 |
//! | `shortener` | a URL shortener hiding the destination | 0.25 |
//! | `risky_tld` | a TLD heavily used for abuse (`.xyz`, `.tk`...) | 0.2 |
//! | `ip_link` | a link to a bare IP address | 0.35 |
//! | `contact_offsite` | "write me on Telegram/WhatsApp", phone-like numbers | 0.3 |
//! | `prize` | "you won", giveaway winner, claim your prize | 0.35 |
//! | `crypto` | investment / doubling / wallet seed talk | 0.3 |
//! | `free_currency` | free Robux, V-Bucks, Nitro, skins, gift cards | 0.35 |
//! | `credentials` | "verify your account", "log in to claim" | 0.3 |
//! | `impersonation` | the author's name imitates the creator's | 0.4 |
//! | `urgency` | "only today", "hurry", "last chance" | 0.1 |
//!
//! Score = `1 - Π(1 - wᵢ)` (independent signals; any single strong one can't reach 1).
//! Known-good domains (youtube.com, youtu.be, twitch.tv, ...) never count as lookalikes.

use crate::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use specta::Type;

pub const SUSPICIOUS: f64 = 0.5;

/// Brands people imitate in creator comments, with their real registrable domains.
const BRANDS: &[(&str, &[&str])] = &[
    ("youtube", &["youtube.com", "youtu.be"]),
    ("google", &["google.com"]),
    ("twitch", &["twitch.tv"]),
    ("instagram", &["instagram.com"]),
    ("tiktok", &["tiktok.com"]),
    ("discord", &["discord.com", "discord.gg", "discordapp.com"]),
    ("steam", &["steampowered.com", "steamcommunity.com"]),
    ("steamcommunity", &["steamcommunity.com"]),
    ("paypal", &["paypal.com", "paypal.me"]),
    ("roblox", &["roblox.com"]),
    ("minecraft", &["minecraft.net"]),
    ("mojang", &["mojang.com"]),
    ("epicgames", &["epicgames.com"]),
    ("fortnite", &["fortnite.com"]),
    ("binance", &["binance.com"]),
    ("metamask", &["metamask.io"]),
    ("facebook", &["facebook.com"]),
    ("whatsapp", &["whatsapp.com", "wa.me"]),
    ("telegram", &["telegram.org", "t.me"]),
    ("amazon", &["amazon.com", "amazon.es"]),
    ("netflix", &["netflix.com"]),
    ("spotify", &["spotify.com"]),
    ("apple", &["apple.com"]),
    ("microsoft", &["microsoft.com"]),
    ("xbox", &["xbox.com"]),
    ("playstation", &["playstation.com"]),
    ("nintendo", &["nintendo.com"]),
];

const SHORTENERS: &[&str] = &[
    "bit.ly",
    "tinyurl.com",
    "t.co",
    "goo.gl",
    "is.gd",
    "cutt.ly",
    "rb.gy",
    "shorturl.at",
    "ow.ly",
    "buff.ly",
    "tiny.cc",
    "v.gd",
    "s.id",
    "rebrand.ly",
    "linktr.ee",
    "lnkd.in",
    "shorturl.com",
    "t.ly",
    "bl.ink",
    "short.io",
];

const RISKY_TLDS: &[&str] = &[
    "xyz", "top", "click", "tk", "ml", "ga", "cf", "gq", "icu", "monster", "rest", "buzz", "cam",
    "sbs", "cfd", "quest", "zip", "mov", "lol", "fun", "work", "loan", "gift",
];

/// Common TLDs recognised in bare domains (without them "hola.que" would be a link).
const KNOWN_TLDS: &[&str] = &[
    "com", "net", "org", "io", "gg", "tv", "me", "be", "es", "co", "app", "dev", "info", "biz",
    "ru", "cn", "us", "uk", "de", "fr", "it", "mx", "ar", "cl", "pe", "br", "eu", "online", "site",
    "store", "shop", "live", "link", "pro", "club", "vip", "win", "bet", "id", "ly", "at", "cc",
    "gd", "ee", "in", "to", "sh", "ai",
];

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub score: f64,
    pub reasons: Vec<String>,
    /// Domains found (after de-obfuscation).
    pub domains: Vec<String>,
}

/// Unicode confusables → ASCII skeleton (a practical subset of UTS #39), plus the classic
/// digit and letter swaps (`0→o`, `1→l`, `rn→m`, `vv→w`).
pub fn skeleton(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        out.push(match c {
            // Cyrillic
            'а' => 'a',
            'в' => 'b',
            'е' | 'ё' => 'e',
            'к' => 'k',
            'м' => 'm',
            'н' => 'h',
            'о' => 'o',
            'р' => 'p',
            'с' => 'c',
            'т' => 't',
            'у' => 'y',
            'х' => 'x',
            'і' => 'i',
            'ј' => 'j',
            'ѕ' => 's',
            'ԁ' => 'd',
            'ӏ' => 'l',
            'ԛ' => 'q',
            'ԝ' => 'w',
            // Greek
            'α' => 'a',
            'β' => 'b',
            'ε' => 'e',
            'η' => 'n',
            'ι' => 'i',
            'κ' => 'k',
            'ν' => 'v',
            'ο' => 'o',
            'ρ' => 'p',
            'τ' => 't',
            'υ' => 'u',
            'χ' => 'x',
            'ω' => 'w',
            // Latin look-alikes and accents
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ą' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'ę' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'ı' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            'ß' => 's',
            'ŀ' | 'ł' => 'l',
            // Digits and symbols
            '0' => 'o',
            '1' | '|' => 'l',
            '3' => 'e',
            '4' | '@' => 'a',
            '5' | '$' => 's',
            '7' => 't',
            '8' => 'b',
            '9' => 'g',
            c => c,
        });
    }
    out.replace("rn", "m").replace("vv", "w").replace("cl", "d")
}

/// Optimal string alignment distance (Damerau-Levenshtein with adjacent transpositions).
pub fn damerau(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[n][m]
}

/// Undoes common link obfuscations: `hxxp`, `[.]`, `(.)`, `(dot)`, `[dot]`, ` dot `,
/// spaced dots (`t . me`), and fullwidth dots.
pub fn deobfuscate(text: &str) -> (String, bool) {
    let lower = text.to_lowercase();
    let mut s = lower
        .replace("hxxps", "https")
        .replace("hxxp", "http")
        .replace("[.]", ".")
        .replace("(.)", ".")
        .replace("{.}", ".")
        .replace("[dot]", ".")
        .replace("(dot)", ".")
        .replace("{dot}", ".")
        .replace("[punto]", ".")
        .replace("(punto)", ".")
        .replace(['。', '．'], ".");
    // " dot " / " punto " between two words that look like a domain.
    for w in [" dot ", " punto "] {
        s = s.replace(w, ".");
    }
    // Spaced dots: "t . me" → "t.me" (only letter-dot-letter, so prose is untouched).
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' || chars[i] == '.' {
            // Look for `<alnum> {spaces} . {spaces} <alnum>` with at least one space.
            let start = i;
            let mut j = i;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            if j < chars.len() && chars[j] == '.' {
                let mut k = j + 1;
                while k < chars.len() && chars[k] == ' ' {
                    k += 1;
                }
                let spaced = j > start || k > j + 1;
                let before = out.chars().last().is_some_and(|c| c.is_alphanumeric());
                let after = chars.get(k).is_some_and(|c| c.is_alphanumeric());
                // The TLD after the dot must be short and alphabetic.
                let tld: String = chars[k..]
                    .iter()
                    .take_while(|c| c.is_alphanumeric())
                    .collect();
                if spaced
                    && before
                    && after
                    && (KNOWN_TLDS.contains(&tld.as_str()) || RISKY_TLDS.contains(&tld.as_str()))
                {
                    out.push('.');
                    i = k;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    let changed = out != lower;
    (out, changed)
}

/// Hostnames in `text` (after de-obfuscation): from URLs and bare `name.tld` tokens.
pub fn extract_domains(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split(|c: char| c.is_whitespace() || "<>\"'()[]{},;!¡¿".contains(c)) {
        let w = raw.trim_matches(|c: char| !c.is_alphanumeric());
        if w.is_empty() {
            continue;
        }
        let rest = w
            .strip_prefix("https://")
            .or_else(|| w.strip_prefix("http://"))
            .unwrap_or(w);
        let had_scheme = rest.len() != w.len();
        let host = rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("")
            .rsplit('@')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .trim_start_matches("www.")
            .trim_end_matches('.');
        if !host.contains('.') || host.starts_with('.') {
            continue;
        }
        let tld = host.rsplit('.').next().unwrap_or("");
        let is_ip =
            host.split('.').count() == 4 && host.split('.').all(|p| p.parse::<u8>().is_ok());
        let plausible = is_ip
            || tld.starts_with("xn--")
            || (tld.chars().all(|c| c.is_alphabetic())
                && (2..=12).contains(&tld.chars().count())
                && (had_scheme || KNOWN_TLDS.contains(&tld) || RISKY_TLDS.contains(&tld)));
        let host = host.to_lowercase();
        if plausible && !out.contains(&host) {
            out.push(host);
        }
    }
    out
}

fn registrable(host: &str) -> String {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() >= 3
        && matches!(parts[parts.len() - 2], "co" | "com" | "org" | "net")
        && parts[parts.len() - 1].len() == 2
    {
        parts[parts.len() - 3..].join(".")
    } else if parts.len() >= 2 {
        parts[parts.len() - 2..].join(".")
    } else {
        host.to_owned()
    }
}

fn is_official(host: &str) -> bool {
    let reg = registrable(host);
    BRANDS
        .iter()
        .flat_map(|(_, ds)| ds.iter())
        .any(|d| reg == *d || host == *d)
}

/// The brand a host imitates, if any (never for the brand's own domains).
pub fn lookalike_of(host: &str) -> Option<&'static str> {
    if is_official(host) {
        return None;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let main = labels
        .get(labels.len().wrapping_sub(2))
        .copied()
        .unwrap_or(host);
    let candidates: Vec<String> = labels[..labels.len().saturating_sub(1)]
        .iter()
        .map(|l| l.to_string())
        .chain(std::iter::once(main.replace('-', "")))
        .chain(main.split('-').map(str::to_owned))
        .collect();
    // Longest brands first: "steamcommunity" before "steam".
    let mut brands: Vec<&'static str> = BRANDS.iter().map(|b| b.0).collect();
    brands.sort_by_key(|b| std::cmp::Reverse(b.len()));
    for brand in brands {
        for c in &candidates {
            if c.is_empty() {
                continue;
            }
            let sk = skeleton(c);
            // The brand inside another word: "youtube-gift", "free-robux-roblox".
            let contains = c.len() > brand.len() && (c.contains(brand) || sk.contains(brand));
            let dist = damerau(&sk, brand);
            let close = brand.len() >= 5 && dist <= if brand.len() >= 8 { 2 } else { 1 };
            if sk == brand || contains || close {
                return Some(brand);
            }
        }
    }
    None
}

fn has_any(text: &str, terms: &[&str]) -> bool {
    terms.iter().any(|t| text.contains(t))
}

/// Phone-like: 9+ digits, allowing spaces, dashes and a leading `+`.
fn has_phone(text: &str) -> bool {
    let mut digits = 0;
    let mut run = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            digits += 1;
            run.push(c);
        } else if c == ' ' || c == '-' || c == '+' || c == '.' {
            continue;
        } else {
            if digits >= 9 {
                return true;
            }
            digits = 0;
            run.clear();
        }
    }
    digits >= 9
}

/// Scores one comment. `creator_names` are the creator's own names/handles, used to spot
/// authors impersonating them (pass the comment author as `author`).
pub fn analyse(text: &str, author: Option<&str>, creator_names: &[String]) -> Verdict {
    let mut reasons: Vec<(&'static str, f64)> = Vec::new();
    let add = |r: &mut Vec<(&'static str, f64)>, code: &'static str, w: f64| {
        if !r.iter().any(|x| x.0 == code) {
            r.push((code, w));
        }
    };
    let (clean, obfuscated) = deobfuscate(text);
    let domains = extract_domains(&clean);
    let plain_domains = extract_domains(&text.to_lowercase());
    if obfuscated && domains.len() > plain_domains.len() {
        add(&mut reasons, "obfuscated_link", 0.35);
    }
    for d in &domains {
        if d.contains("xn--") {
            add(&mut reasons, "punycode", 0.4);
        }
        if d.split('.').count() == 4 && d.split('.').all(|p| p.parse::<u8>().is_ok()) {
            add(&mut reasons, "ip_link", 0.35);
            continue;
        }
        if SHORTENERS.contains(&d.as_str()) || SHORTENERS.contains(&registrable(d).as_str()) {
            add(&mut reasons, "shortener", 0.25);
        }
        if RISKY_TLDS.contains(&d.rsplit('.').next().unwrap_or("")) {
            add(&mut reasons, "risky_tld", 0.2);
        }
        if lookalike_of(d).is_some() {
            add(&mut reasons, "lookalike_domain", 0.6);
        }
    }

    let t = crate::community::classify::normalize(text);
    // Raw lowercase too: normalize() folds some symbols the patterns rely on.
    let raw = text.to_lowercase();
    if has_any(
        &t,
        &[
            "telegram",
            "whatsapp",
            "wa.me",
            "t.me",
            "escribeme",
            "escribe me",
            "contactame",
            "contact me",
            "dm me",
            "mandame un dm",
            "message me",
            "text me",
            "hablame",
        ],
    ) && (has_any(
        &t,
        &[
            "escrib", "contact", "dm", "habla", "message", "text me", "manda", "via", "por ",
        ],
    ) || raw.contains("t.me")
        || raw.contains("wa.me"))
        || has_phone(text)
    {
        add(&mut reasons, "contact_offsite", 0.3);
    }
    if has_any(
        &t,
        &[
            "has ganado",
            "has sido seleccionado",
            "eres el ganador",
            "ganador del sorteo",
            "you won",
            "you have won",
            "you ve won",
            "youve won",
            "winner",
            "congratulations you",
            "felicidades has",
            "reclama tu premio",
            "claim your prize",
            "claim your reward",
            "premio",
            "giveaway",
            "sorteo",
        ],
    ) && has_any(
        &t,
        &[
            "ganad",
            "won",
            "winner",
            "seleccionad",
            "reclama",
            "claim",
            "premio",
            "prize",
            "reward",
            "contact",
            "escrib",
            "telegram",
            "whatsapp",
            "dm",
            "link",
            "enlace",
            "click",
        ],
    ) {
        add(&mut reasons, "prize", 0.35);
    }
    if has_any(
        &t,
        &[
            "bitcoin",
            "btc",
            "ethereum",
            "usdt",
            "crypto",
            "cripto",
            "binance",
            "forex",
            "invierte",
            "inversion",
            "investment",
            "invest ",
            "trading",
            "duplica",
            "doblar tu dinero",
            "double your",
            "seed phrase",
            "frase semilla",
            "wallet",
            "airdrop",
            "rentabilidad",
            "ganancias diarias",
            "daily profit",
        ],
    ) {
        add(&mut reasons, "crypto", 0.3);
    }
    if has_any(
        &t,
        &[
            "free robux",
            "robux gratis",
            "free vbucks",
            "free v bucks",
            "pavos gratis",
            "vbucks gratis",
            "free nitro",
            "nitro gratis",
            "discord nitro",
            "skins gratis",
            "free skins",
            "gift card",
            "tarjeta regalo",
            "steam gift",
            "minecoins gratis",
            "free minecoins",
        ],
    ) {
        add(&mut reasons, "free_currency", 0.35);
    }
    if has_any(
        &t,
        &[
            "verify your account",
            "verifica tu cuenta",
            "confirma tu cuenta",
            "confirm your account",
            "inicia sesion para",
            "log in to claim",
            "login to claim",
            "your account will be",
            "tu cuenta sera",
            "suspended",
            "suspendida",
            "copyright strike",
            "strike de copyright",
        ],
    ) {
        add(&mut reasons, "credentials", 0.3);
    }
    if has_any(
        &t,
        &[
            "solo hoy",
            "only today",
            "ultima oportunidad",
            "last chance",
            "date prisa",
            "hurry",
            "limited time",
            "tiempo limitado",
            "24 horas",
            "24 hours",
        ],
    ) {
        add(&mut reasons, "urgency", 0.1);
    }
    if let Some(a) = author {
        if impersonates(a, creator_names) {
            add(&mut reasons, "impersonation", 0.4);
        }
    }

    let score = 1.0 - reasons.iter().fold(1.0, |acc, (_, w)| acc * (1.0 - w));
    Verdict {
        score: (score * 1000.0).round() / 1000.0,
        reasons: reasons.into_iter().map(|r| r.0.to_owned()).collect(),
        domains,
    }
}

/// `name` imitates one of `creator_names` (not identical: identical is the creator).
pub fn impersonates(name: &str, creator_names: &[String]) -> bool {
    name_similarity_max(name, creator_names).is_some_and(|(s, exact)| !exact && s >= 0.8)
}

/// Best similarity (0..1) between `name` and any creator name, and whether it was an
/// exact (case-insensitive, `@`-less) match.
pub fn name_similarity_max(name: &str, creator_names: &[String]) -> Option<(f64, bool)> {
    let norm = |s: &str| -> String {
        s.trim()
            .trim_start_matches('@')
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || !c.is_ascii())
            .collect()
    };
    let n = norm(name);
    if n.chars().count() < 4 {
        return None;
    }
    creator_names
        .iter()
        .map(|c| norm(c))
        .filter(|c| c.chars().count() >= 4)
        .map(|c| {
            if c == n {
                return (1.0, true);
            }
            let (a, b) = (skeleton(&n), skeleton(&c));
            let len = a.chars().count().max(b.chars().count()) as f64;
            let sim = 1.0 - damerau(&a, &b) as f64 / len;
            // "gh0stymc_oficial", "gh0stymc.support": the creator's name plus a suffix.
            let contains = a.contains(&b) && a.len() <= b.len() + 12;
            (if contains { sim.max(0.85) } else { sim }, false)
        })
        .max_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal))
}

/// Scores the given comments and stores the result. The creator's names come from the
/// connected accounts (display names and handles).
pub fn apply(conn: &Connection, ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    let names: Vec<String> = {
        let mut st = conn.prepare("SELECT display_name, handle FROM accounts")?;
        let r = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        r.collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .flat_map(|(d, h)| std::iter::once(d).chain(h))
            .collect()
    };
    let mut get = conn.prepare("SELECT text, author_name, is_own FROM comments WHERE id = ?1")?;
    let mut set = conn
        .prepare("UPDATE comments SET phishing_score = ?2, phishing_reasons = ?3 WHERE id = ?1")?;
    for id in ids {
        let row: Option<(String, Option<String>, bool)> = get
            .query_row(params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                e => Err(e),
            })?;
        let Some((text, author, own)) = row else {
            continue;
        };
        if own {
            continue;
        }
        let v = analyse(&text, author.as_deref(), &names);
        set.execute(params![
            id,
            v.score,
            serde_json::to_string(&v.reasons).unwrap_or_else(|_| "[]".into())
        ])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creator() -> Vec<String> {
        vec!["gh0stymc".into(), "@gh0stymc".into()]
    }

    fn v(text: &str) -> Verdict {
        analyse(text, None, &creator())
    }

    fn has(v: &Verdict, code: &str) -> bool {
        v.reasons.iter().any(|r| r == code)
    }

    // ---- primitives ----

    #[test]
    fn damerau_distance() {
        assert_eq!(damerau("youtube", "youtube"), 0);
        assert_eq!(damerau("youtbue", "youtube"), 1, "transposition");
        assert_eq!(damerau("yotube", "youtube"), 1, "deletion");
        assert_eq!(damerau("youtubee", "youtube"), 1, "insertion");
        assert_eq!(damerau("yautube", "youtube"), 1, "substitution");
        assert_eq!(damerau("", "abc"), 3);
        assert_eq!(damerau("abc", ""), 3);
    }

    #[test]
    fn skeleton_folds_confusables() {
        assert_eq!(skeleton("Y0UTUBE"), "youtube");
        assert_eq!(skeleton("уоutubе"), "youtube", "cyrillic у, о, е");
        assert_eq!(skeleton("rnicrosoft"), "microsoft");
        assert_eq!(skeleton("paypa1"), "paypal");
        assert_eq!(skeleton("dlscord"), "dlscord");
        assert_eq!(skeleton("twítch"), "twitch");
        assert_eq!(skeleton("ѕteam"), "steam");
        assert_eq!(skeleton("vvhatsapp"), "whatsapp");
    }

    #[test]
    fn deobfuscation() {
        assert_eq!(deobfuscate("hxxps://evil[.]com").0, "https://evil.com");
        assert_eq!(
            deobfuscate("entra en t . me/canal").0,
            "entra en t.me/canal"
        );
        assert_eq!(deobfuscate("bit (dot) ly/abc").0, "bit.ly/abc");
        assert_eq!(deobfuscate("discord dot gg/x").0, "discord.gg/x");
        assert_eq!(deobfuscate("premios。xyz").0, "premios.xyz");
        // Normal prose is untouched.
        let (s, changed) = deobfuscate("Hola. Qué tal . bien");
        assert_eq!(s, "hola. qué tal . bien");
        assert!(!changed);
        assert!(!deobfuscate("1 . 5 veces").1);
    }

    #[test]
    fn domain_extraction() {
        assert_eq!(
            extract_domains("mira https://www.Example.com/path?x=1"),
            vec!["example.com"]
        );
        assert_eq!(
            extract_domains("entra en premios.xyz ya"),
            vec!["premios.xyz"]
        );
        assert_eq!(extract_domains("(t.me/canal)"), vec!["t.me"]);
        assert_eq!(
            extract_domains("http://192.168.1.10:8080/login"),
            vec!["192.168.1.10"]
        );
        assert_eq!(extract_domains("http://user@evil.com/"), vec!["evil.com"]);
        assert!(
            extract_domains("hola.que tal").is_empty(),
            "unknown TLD without scheme"
        );
        assert!(extract_domains("1.5 veces mejor, fin.").is_empty());
        assert!(extract_domains("v1.2.3").is_empty());
        assert_eq!(extract_domains("a.com a.com").len(), 1, "deduplicated");
    }

    #[test]
    fn lookalikes_and_officials() {
        for d in [
            "youtube.com",
            "m.youtube.com",
            "youtu.be",
            "twitch.tv",
            "discord.gg",
            "steamcommunity.com",
            "paypal.me",
            "t.me",
            "wa.me",
        ] {
            assert_eq!(lookalike_of(d), None, "{d} is official");
        }
        for (d, b) in [
            ("y0utube.com", "youtube"),
            ("youtubee.net", "youtube"),
            ("youtube-gift.xyz", "youtube"),
            ("steamcomrnunity.com", "steamcommunity"),
            ("stearncommunity.ru", "steamcommunity"),
            ("discorcl.gift", "discord"),
            ("dlscord-nitro.com", "discord"),
            ("paypa1.com", "paypal"),
            ("free-robux-roblox.top", "roblox"),
            ("twltch.tv", "twitch"),
            ("rninecraft.net", "minecraft"),
            ("login.instagrarn.com", "instagram"),
            ("binance-airdrop.site", "binance"),
        ] {
            assert_eq!(lookalike_of(d), Some(b), "{d}");
        }
        for d in [
            "example.com",
            "github.com",
            "wikipedia.org",
            "elpais.es",
            "steam.es",
        ] {
            // steam.es: same label as a brand but not its domain → lookalike.
            if d == "steam.es" {
                assert_eq!(lookalike_of(d), Some("steam"));
            } else {
                assert_eq!(lookalike_of(d), None, "{d}");
            }
        }
    }

    #[test]
    fn phones() {
        assert!(has_phone("llámame al +34 612 345 678"));
        assert!(has_phone("612-345-678"));
        assert!(!has_phone("tengo 25 años y 3 gatos"));
        assert!(!has_phone("minuto 12:30"));
    }

    // ---- full verdicts: scams ----

    #[test]
    fn classic_scams_are_suspicious() {
        let cases = [
            "¡Felicidades! Has ganado el sorteo de gh0stymc 🎉 escríbeme por Telegram para reclamar tu premio",
            "Congratulations you won the giveaway! Claim your prize at bit.ly/3xYz",
            "Free robux at robl0x-gift.com click now",
            "Gana dinero desde casa con bitcoin, invierte 100€ y duplica en 24 horas, whatsapp +34 612 345 678",
            "free discord nitro: discorcl.gift/abc",
            "Tu cuenta será suspendida, verifica tu cuenta en hxxps://youtube-support[.]xyz",
            "steam gift card 👉 steamcomrnunity.com/gift/123",
            "Entra en t . me/premios_gh0sty, solo hoy",
            "airdrop de usdt, conecta tu wallet en binance-airdrop.site",
            "claim your reward: http://45.12.3.9/claim",
        ];
        for c in cases {
            let r = v(c);
            assert!(r.score >= SUSPICIOUS, "{c} → {r:?}");
            assert!(!r.reasons.is_empty());
        }
    }

    #[test]
    fn specific_reasons() {
        assert!(has(&v("visita y0utube.com/login"), "lookalike_domain"));
        assert!(has(&v("mira bit.ly/abc"), "shortener"));
        assert!(has(&v("entra en ofertas.xyz"), "risky_tld"));
        assert!(has(&v("hxxp://evil[.]com"), "obfuscated_link"));
        assert!(has(&v("https://xn--yutube-wqf.com"), "punycode"));
        assert!(has(&v("escríbeme por telegram"), "contact_offsite"));
        assert!(has(&v("free vbucks here"), "free_currency"));
        assert!(has(&v("invierte en cripto y duplica"), "crypto"));
        assert!(has(&v("verify your account now"), "credentials"));
        assert!(has(&v("last chance!!"), "urgency"));
        assert!(has(&v("has ganado un premio"), "prize"));
        assert!(has(&v("http://10.0.0.1/a"), "ip_link"));
    }

    #[test]
    fn impersonating_authors() {
        let names = creator();
        assert!(analyse("hola", Some("gh0styrnc"), &names)
            .reasons
            .contains(&"impersonation".to_string()));
        assert!(analyse("hola", Some("gh0stymc_oficial"), &names)
            .reasons
            .contains(&"impersonation".to_string()));
        assert!(analyse("hola", Some("ghostymc"), &names)
            .reasons
            .contains(&"impersonation".to_string()));
        // The creator themself and unrelated names are fine.
        assert!(analyse("hola", Some("gh0stymc"), &names).reasons.is_empty());
        assert!(analyse("hola", Some("@GH0STYMC"), &names)
            .reasons
            .is_empty());
        assert!(analyse("hola", Some("Marta"), &names).reasons.is_empty());
        assert!(
            analyse("hola", Some("abc"), &names).reasons.is_empty(),
            "too short to judge"
        );
        // Impersonator + contact request = clearly a scam.
        let r = analyse(
            "Has ganado! escríbeme por telegram",
            Some("gh0stymc_premios"),
            &names,
        );
        assert!(r.score >= 0.7, "{r:?}");
    }

    // ---- full verdicts: legitimate comments ----

    #[test]
    fn normal_comments_are_clean() {
        let cases = [
            "¡Me encanta este vídeo, sigue así!",
            "¿Qué mod usas para las sombras?",
            "Minuto 12:30 jajaja",
            "Mira mi canal de YouTube https://youtube.com/@otro",
            "El enlace está en https://youtu.be/dQw4w9WgXcQ",
            "Únete al discord: discord.gg/minecraft",
            "Lo compré en steamcommunity.com",
            "1.5 veces más rápido. Fin.",
            "who else is watching in 2026?",
            "el sorteo del otro día fue genial",
            "Tengo 3 gatos y 2 perros",
            "Paga con paypal.me/gh0sty si quieres apoyar",
            "gg wp",
            "",
        ];
        for c in cases {
            let r = v(c);
            assert!(r.score < SUSPICIOUS, "{c} → {r:?}");
        }
        // Completely clean ones have no reasons at all.
        for c in [
            "¡Me encanta este vídeo, sigue así!",
            "¿Qué mod usas?",
            "gg wp",
        ] {
            assert!(v(c).reasons.is_empty(), "{c}");
        }
    }

    #[test]
    fn score_combines_independently_and_stays_below_one() {
        let single = v("mira bit.ly/abc").score;
        assert!((single - 0.25).abs() < 1e-9);
        let many = v("Has ganado!! free robux, escríbeme por telegram, entra en y0utube-gift.xyz bit.ly/x solo hoy");
        assert!(many.score > 0.9 && many.score < 1.0, "{many:?}");
        let mut uniq = many.reasons.clone();
        uniq.dedup();
        assert_eq!(uniq.len(), many.reasons.len(), "no duplicated reasons");
    }

    #[test]
    fn stored_verdicts() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (id, platform, external_id, display_name, handle, connected_at)
               VALUES ('acc', 'youtube', 'UC1', 'gh0stymc', '@gh0stymc', 't');
             INSERT INTO comments (id, account_id, external_id, text, author_name, fetched_at)
               VALUES ('c1', 'acc', 'e1', 'Free robux at robl0x-gift.com', 'x', 't'),
                      ('c2', 'acc', 'e2', 'buen vídeo', 'gh0stymc_oficial', 't'),
                      ('c3', 'acc', 'e3', 'gracias', 'Marta', 't');",
        )
        .unwrap();
        apply(
            &conn,
            &["c1".into(), "c2".into(), "c3".into(), "missing".into()],
        )
        .unwrap();
        let get = |id: &str| -> (f64, String) {
            conn.query_row(
                "SELECT phishing_score, phishing_reasons FROM comments WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
        };
        assert!(get("c1").0 >= SUSPICIOUS);
        assert!(get("c2").1.contains("impersonation"));
        assert_eq!(get("c3"), (0.0, "[]".into()));
    }
}
