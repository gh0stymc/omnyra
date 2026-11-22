//! Creation tools (phase 5): the creator's asset library (skins, avatars, stickers...),
//! thumbnail templates, exported thumbnails and text templates.
//!
//! Library files are *copied* into `app_data_dir/files/` (managed): the user's originals
//! can move or disappear without breaking templates. Images reach the webview as data
//! URLs (the CSP only allows `self`, `asset:` and `data:`).

use crate::{AppError, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

pub const ASSET_KINDS: &[&str] = &["avatar", "skin", "sticker", "background", "image"];
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];
const MAX_ASSET_BYTES: u64 = 20 * 1024 * 1024;
/// Exported thumbnails: PNG or JPEG, at most 20 MB of base64-decoded data.
const MAX_EXPORT_BYTES: usize = 20 * 1024 * 1024;

fn validation(field: &str) -> AppError {
    AppError::Validation {
        field: field.into(),
    }
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn mime_of(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "application/octet-stream",
    }
}

// ---- assets --------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub variant: Option<String>,
    pub tags: Vec<String>,
    pub mime: String,
    pub size_bytes: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AssetInput {
    pub kind: String,
    pub name: String,
    /// "normal", "halloween", "navidad"...
    pub variant: Option<String>,
    pub tags: Vec<String>,
}

fn check_asset_input(i: &AssetInput) -> Result<()> {
    if !ASSET_KINDS.contains(&i.kind.as_str()) {
        return Err(validation("kind"));
    }
    if i.name.trim().is_empty() || i.name.chars().count() > 120 {
        return Err(validation("name"));
    }
    Ok(())
}

/// Copies `source` into the library (`files_dir/<aa>/<uuid>.<ext>`).
pub fn import_asset(
    conn: &Connection,
    files_dir: &Path,
    source: &Path,
    input: &AssetInput,
    now: &str,
) -> Result<Asset> {
    check_asset_input(input)?;
    let ext = ext_of(source);
    if !IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        return Err(validation("file"));
    }
    let meta = std::fs::metadata(source).map_err(|_| AppError::FileUnavailable)?;
    if !meta.is_file() {
        return Err(AppError::FileUnavailable);
    }
    if meta.len() > MAX_ASSET_BYTES {
        return Err(validation("file"));
    }
    let file_id = uuid::Uuid::now_v7().to_string();
    let dir = files_dir.join(&file_id[file_id.len() - 2..]);
    std::fs::create_dir_all(&dir).map_err(|_| AppError::FileUnavailable)?;
    let dest = dir.join(format!("{file_id}.{ext}"));
    std::fs::copy(source, &dest).map_err(|_| AppError::FileUnavailable)?;
    let name = source.file_name().map(|n| n.to_string_lossy().into_owned());
    let id = uuid::Uuid::now_v7().to_string();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO files (id, path, managed, original_name, mime, size_bytes, created_at)
         VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6)",
        params![
            file_id,
            dest.to_string_lossy(),
            name,
            mime_of(&ext),
            meta.len() as i64,
            now
        ],
    )?;
    tx.execute(
        "INSERT INTO assets (id, file_id, kind, name, variant, tags, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            file_id,
            input.kind,
            input.name.trim(),
            input
                .variant
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty()),
            serde_json::to_string(&input.tags).map_err(|_| AppError::Internal)?,
            now
        ],
    )?;
    if let Err(e) = tx.commit() {
        let _ = std::fs::remove_file(&dest);
        return Err(e.into());
    }
    get_asset(conn, &id)
}

const ASSET_SQL: &str = "SELECT a.id, a.kind, a.name, a.variant, a.tags, f.mime, f.size_bytes,
                                a.created_at
                         FROM assets a JOIN files f ON f.id = a.file_id";

fn asset_row(r: &rusqlite::Row) -> rusqlite::Result<Asset> {
    let tags: String = r.get(4)?;
    Ok(Asset {
        id: r.get(0)?,
        kind: r.get(1)?,
        name: r.get(2)?,
        variant: r.get(3)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        mime: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
        size_bytes: r.get::<_, Option<i64>>(6)?.unwrap_or(0) as f64,
        created_at: r.get(7)?,
    })
}

pub fn get_asset(conn: &Connection, id: &str) -> Result<Asset> {
    conn.query_row(
        &format!("{ASSET_SQL} WHERE a.id = ?1"),
        params![id],
        asset_row,
    )
    .optional()?
    .ok_or(AppError::NotFound)
}

pub fn list_assets(conn: &Connection) -> Result<Vec<Asset>> {
    let mut st = conn.prepare(&format!("{ASSET_SQL} ORDER BY a.kind, a.name, a.variant"))?;
    let rows = st.query_map([], asset_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn update_asset(conn: &Connection, id: &str, input: &AssetInput) -> Result<Asset> {
    check_asset_input(input)?;
    let n = conn.execute(
        "UPDATE assets SET kind = ?2, name = ?3, variant = ?4, tags = ?5 WHERE id = ?1",
        params![
            id,
            input.kind,
            input.name.trim(),
            input
                .variant
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty()),
            serde_json::to_string(&input.tags).map_err(|_| AppError::Internal)?
        ],
    )?;
    if n == 0 {
        return Err(AppError::NotFound);
    }
    get_asset(conn, id)
}

/// Deletes the asset and its managed copy.
pub fn delete_asset(conn: &Connection, id: &str) -> Result<()> {
    let (file_id, path): (String, String) = conn
        .query_row(
            "SELECT f.id, f.path FROM assets a JOIN files f ON f.id = a.file_id WHERE a.id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or(AppError::NotFound)?;
    conn.execute(
        "DELETE FROM files WHERE id = ?1 AND managed = 1",
        params![file_id],
    )?;
    let _ = std::fs::remove_file(path);
    Ok(())
}

/// `data:<mime>;base64,...` of an asset, for the editor canvas.
pub fn asset_data_url(conn: &Connection, id: &str) -> Result<String> {
    let (path, mime): (String, Option<String>) = conn
        .query_row(
            "SELECT f.path, f.mime FROM assets a JOIN files f ON f.id = a.file_id WHERE a.id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or(AppError::NotFound)?;
    let bytes = std::fs::read(&path).map_err(|_| AppError::FileUnavailable)?;
    Ok(format!(
        "data:{};base64,{}",
        mime.unwrap_or_else(|| "application/octet-stream".into()),
        STANDARD.encode(bytes)
    ))
}

// ---- thumbnail templates ---------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailTemplate {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Editor scene (layers) as JSON; the frontend owns its shape.
    pub scene: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TemplateInput {
    /// Existing template to overwrite; `None` creates one.
    pub id: Option<String>,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub scene: String,
}

pub fn save_template(conn: &Connection, t: &TemplateInput, now: &str) -> Result<ThumbnailTemplate> {
    if t.name.trim().is_empty() || t.name.chars().count() > 120 {
        return Err(validation("name"));
    }
    if !(16..=4096).contains(&t.width) || !(16..=4096).contains(&t.height) {
        return Err(validation("size"));
    }
    if t.scene.len() > 5 * 1024 * 1024
        || serde_json::from_str::<serde_json::Value>(&t.scene).is_err()
    {
        return Err(validation("scene"));
    }
    let id = match &t.id {
        Some(id) => {
            let n = conn.execute(
                "UPDATE thumbnail_templates SET name = ?2, width = ?3, height = ?4, scene = ?5,
                        updated_at = ?6 WHERE id = ?1",
                params![id, t.name.trim(), t.width, t.height, t.scene, now],
            )?;
            if n == 0 {
                return Err(AppError::NotFound);
            }
            id.clone()
        }
        None => {
            let id = uuid::Uuid::now_v7().to_string();
            conn.execute(
                "INSERT INTO thumbnail_templates (id, name, width, height, scene, created_at,
                                                  updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![id, t.name.trim(), t.width, t.height, t.scene, now],
            )?;
            id
        }
    };
    conn.query_row(
        "SELECT id, name, width, height, scene, updated_at FROM thumbnail_templates
         WHERE id = ?1",
        params![id],
        template_row,
    )
    .map_err(Into::into)
}

fn template_row(r: &rusqlite::Row) -> rusqlite::Result<ThumbnailTemplate> {
    Ok(ThumbnailTemplate {
        id: r.get(0)?,
        name: r.get(1)?,
        width: r.get(2)?,
        height: r.get(3)?,
        scene: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

pub fn list_templates(conn: &Connection) -> Result<Vec<ThumbnailTemplate>> {
    let mut st = conn.prepare(
        "SELECT id, name, width, height, scene, updated_at FROM thumbnail_templates
         ORDER BY updated_at DESC",
    )?;
    let rows = st.query_map([], template_row)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn delete_template(conn: &Connection, id: &str) -> Result<()> {
    if conn.execute("DELETE FROM thumbnail_templates WHERE id = ?1", params![id])? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

// ---- exports ---------------------------------------------------------------------------

/// Writes an exported thumbnail (`data:image/png|jpeg;base64,...`) to
/// `dir/<name>-<uuid>.<ext>` and returns the path (used as the upload's thumbnail).
pub fn save_export(dir: &Path, data_url: &str, name: &str) -> Result<PathBuf> {
    let (ext, b64) = if let Some(b) = data_url.strip_prefix("data:image/png;base64,") {
        ("png", b)
    } else if let Some(b) = data_url.strip_prefix("data:image/jpeg;base64,") {
        ("jpg", b)
    } else {
        return Err(validation("image"));
    };
    if b64.len() > MAX_EXPORT_BYTES * 4 / 3 + 4 {
        return Err(validation("image"));
    }
    let bytes = STANDARD.decode(b64).map_err(|_| validation("image"))?;
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(60)
        .collect();
    let safe = if safe.trim_matches('-').is_empty() {
        "thumbnail".to_owned()
    } else {
        safe
    };
    std::fs::create_dir_all(dir).map_err(|_| AppError::FileUnavailable)?;
    let id = uuid::Uuid::now_v7().to_string();
    let path = dir.join(format!("{safe}-{}.{ext}", &id[id.len() - 8..]));
    std::fs::write(&path, bytes).map_err(|_| AppError::FileUnavailable)?;
    Ok(path)
}

// ---- text templates (5b) ---------------------------------------------------------------

pub const TEXT_FIELDS: &[&str] = &["title", "description", "hashtags"];

#[derive(Debug, Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextTemplate {
    pub id: String,
    pub platform: Option<String>,
    pub field: String,
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TextTemplateInput {
    pub id: Option<String>,
    pub platform: Option<String>,
    pub field: String,
    pub name: String,
    pub body: String,
}

pub fn save_text_template(
    conn: &Connection,
    t: &TextTemplateInput,
    now: &str,
) -> Result<TextTemplate> {
    if !TEXT_FIELDS.contains(&t.field.as_str()) {
        return Err(validation("field"));
    }
    if t.name.trim().is_empty() || t.name.chars().count() > 120 {
        return Err(validation("name"));
    }
    if t.body.trim().is_empty() || t.body.len() > 10_000 {
        return Err(validation("body"));
    }
    let platform = t.platform.as_deref().filter(|p| !p.is_empty());
    let id = match &t.id {
        Some(id) => {
            if conn.execute(
                "UPDATE text_templates SET platform = ?2, field = ?3, name = ?4, body = ?5
                 WHERE id = ?1",
                params![id, platform, t.field, t.name.trim(), t.body],
            )? == 0
            {
                return Err(AppError::NotFound);
            }
            id.clone()
        }
        None => {
            let id = uuid::Uuid::now_v7().to_string();
            conn.execute(
                "INSERT INTO text_templates (id, platform, field, name, body, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, platform, t.field, t.name.trim(), t.body, now],
            )?;
            id
        }
    };
    Ok(TextTemplate {
        id,
        platform: platform.map(str::to_owned),
        field: t.field.clone(),
        name: t.name.trim().to_owned(),
        body: t.body.clone(),
    })
}

pub fn list_text_templates(conn: &Connection) -> Result<Vec<TextTemplate>> {
    let mut st = conn.prepare(
        "SELECT id, platform, field, name, body FROM text_templates ORDER BY field, name",
    )?;
    let rows = st.query_map([], |r| {
        Ok(TextTemplate {
            id: r.get(0)?,
            platform: r.get(1)?,
            field: r.get(2)?,
            name: r.get(3)?,
            body: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn delete_text_template(conn: &Connection, id: &str) -> Result<()> {
    if conn.execute("DELETE FROM text_templates WHERE id = ?1", params![id])? == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("omnyra-cr-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn assets_are_copied_listed_served_and_deleted() {
        let conn = open_in_memory().unwrap();
        let dir = tmp();
        let src = dir.join("skin navidad.png");
        std::fs::write(&src, b"\x89PNG fake").unwrap();
        let input = AssetInput {
            kind: "skin".into(),
            name: "gh0sty".into(),
            variant: Some("navidad".into()),
            tags: vec!["mc".into()],
        };
        let a = import_asset(&conn, &dir.join("files"), &src, &input, "t").unwrap();
        assert_eq!(
            (a.kind.as_str(), a.variant.as_deref()),
            ("skin", Some("navidad"))
        );
        assert_eq!(a.mime, "image/png");
        // The original can go away: the library keeps its own copy.
        std::fs::remove_file(&src).unwrap();
        let url = asset_data_url(&conn, &a.id).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!(list_assets(&conn).unwrap().len(), 1);
        let renamed = update_asset(
            &conn,
            &a.id,
            &AssetInput {
                name: "Ghost".into(),
                ..input.clone()
            },
        )
        .unwrap();
        assert_eq!(renamed.name, "Ghost");
        let path: String = conn
            .query_row("SELECT path FROM files", [], |r| r.get(0))
            .unwrap();
        delete_asset(&conn, &a.id).unwrap();
        assert!(list_assets(&conn).unwrap().is_empty());
        assert!(!Path::new(&path).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_bad_assets() {
        let conn = open_in_memory().unwrap();
        let dir = tmp();
        let txt = dir.join("a.txt");
        std::fs::write(&txt, b"x").unwrap();
        let input = AssetInput {
            kind: "skin".into(),
            name: "x".into(),
            variant: None,
            tags: vec![],
        };
        assert!(matches!(
            import_asset(&conn, &dir, &txt, &input, "t"),
            Err(AppError::Validation { .. })
        ));
        let bad = AssetInput {
            kind: "weird".into(),
            ..input
        };
        assert!(import_asset(&conn, &dir, &dir.join("a.png"), &bad, "t").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn templates_roundtrip() {
        let conn = open_in_memory().unwrap();
        let t = save_template(
            &conn,
            &TemplateInput {
                id: None,
                name: "Base".into(),
                width: 1280,
                height: 720,
                scene: r#"{"layers":[]}"#.into(),
            },
            "t1",
        )
        .unwrap();
        let t2 = save_template(
            &conn,
            &TemplateInput {
                id: Some(t.id.clone()),
                name: "Base 2".into(),
                width: 1280,
                height: 720,
                scene: r#"{"layers":[1]}"#.into(),
            },
            "t2",
        )
        .unwrap();
        assert_eq!(t2.id, t.id);
        assert_eq!(list_templates(&conn).unwrap()[0].name, "Base 2");
        assert!(save_template(
            &conn,
            &TemplateInput {
                id: None,
                name: "x".into(),
                width: 1,
                height: 720,
                scene: "{}".into()
            },
            "t"
        )
        .is_err());
        assert!(save_template(
            &conn,
            &TemplateInput {
                id: None,
                name: "x".into(),
                width: 100,
                height: 100,
                scene: "not json".into()
            },
            "t"
        )
        .is_err());
        delete_template(&conn, &t.id).unwrap();
        assert!(list_templates(&conn).unwrap().is_empty());
    }

    #[test]
    fn exports_decode_and_sanitize_names() {
        let dir = tmp();
        let png = format!("data:image/png;base64,{}", STANDARD.encode(b"img"));
        let p = save_export(&dir, &png, "../../etc/pass wd").unwrap();
        assert!(p.starts_with(&dir));
        assert_eq!(p.extension().unwrap(), "png");
        assert_eq!(std::fs::read(&p).unwrap(), b"img");
        let jpg = format!("data:image/jpeg;base64,{}", STANDARD.encode(b"j"));
        assert_eq!(
            save_export(&dir, &jpg, "").unwrap().extension().unwrap(),
            "jpg"
        );
        assert!(save_export(&dir, "data:text/html;base64,AA==", "x").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn text_templates_validate_and_list() {
        let conn = open_in_memory().unwrap();
        let t = save_text_template(
            &conn,
            &TextTemplateInput {
                id: None,
                platform: Some("youtube".into()),
                field: "title".into(),
                name: "Serie".into(),
                body: "{{serie}} #{{n}}".into(),
            },
            "t",
        )
        .unwrap();
        assert_eq!(list_text_templates(&conn).unwrap(), vec![t.clone()]);
        assert!(save_text_template(
            &conn,
            &TextTemplateInput {
                id: None,
                platform: None,
                field: "nope".into(),
                name: "x".into(),
                body: "y".into()
            },
            "t"
        )
        .is_err());
        delete_text_template(&conn, &t.id).unwrap();
        assert!(list_text_templates(&conn).unwrap().is_empty());
    }
}
