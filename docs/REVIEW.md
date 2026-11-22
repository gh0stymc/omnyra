# Omnyra — Revisión global (paso 7a)

> 2026-11-22 · `claude-opus-5-5` (el usuario pidió hacer todos los pasos seguidos con Opus en lugar de Fable; sin cambio de modelo automático en las partes de seguridad).
> Alcance: `crates/omnyra-core` (≈ 230 tests), `src-tauri` (capa fina, 100+ commands), frontend React (48 tests), `broker/` (6 tests). ~32.000 líneas.

## Resumen

La arquitectura del 0a se ha mantenido: lógica en `omnyra-core` probada sin Tauri, commands finos, tipos generados con `tauri-specta`, secretos solo en el llavero, CSP sin orígenes externos. Los puntos débiles están en el **borde Tauri** (commands que aceptan rutas del frontend), en **tareas largas sin cancelación** y en pequeñas duplicaciones. Nada bloquea el uso personal.

Prioridad: **A** = aplicar ya (seguridad o pérdida de datos/recursos), **M** = conviene, **B** = deuda menor.

## Seguridad

| # | Prio | Hallazgo | Propuesta |
|---|---|---|---|
| S1 | **A** | `write_thumbnail_file`, `save_media_kit` y `export_rendition` escriben en una ruta que manda el frontend. Con un XSS en el webview se podría escribir en cualquier sitio del usuario (en `write_thumbnail_file`, bytes arbitrarios en base64). | Validar en Rust la extensión del destino (`.png/.jpg`, `.html`, `.mp4`), que el contenido de la miniatura sea PNG/JPEG real (bytes mágicos) y que el destino no esté dentro de `app_data_dir` (no pisar la base de datos). |
| S2 | **A** | `import_asset` y `cut_moment`/`export_rendition` leen una ruta arbitraria. Leer sí es lo esperado (vienen del diálogo), pero `import_asset` devuelve el contenido como data URL: un XSS podría exfiltrar imágenes del disco. | Limitar `import_asset` a imágenes verificadas por bytes mágicos (no solo por extensión). |
| S3 | M | El bloqueo con PIN solo se exige en el backend para responder, publicar y desconectar; el resto de commands responde aunque esté bloqueada (la UI los tapa). | Documentado (D7). Si se quiere más, un middleware de `invoke_handler` que rechace todo salvo `lock_status`/`unlock` mientras esté bloqueada. |
| S4 | B | El broker no limita peticiones por IP. | Cloudflare ya limita abuso grueso; añadir *rate limiting rules* del panel si se publica la app a terceros. |

## Fiabilidad y recursos

| # | Prio | Hallazgo | Propuesta |
|---|---|---|---|
| R1 | **A** | Las versiones renderizadas (`app_data_dir/renders/*.mp4`) se quedan en disco tras publicarse; con varios vídeos largos ocupan GB. | Borrar la versión al publicarse con éxito (ya no hace falta para reintentar). |
| R2 | M | Cancelar una publicación en `rendering` no detiene FFmpeg (el resultado se descarta al terminar). | Guardar el `oneshot` de cancelación como en las subidas y matar el proceso (`kill_on_drop` ya está). |
| R3 | M | Instagram: la subida lee el vídeo entero en memoria (≤ 300 MB). | Subir por trozos con `offset` de `rupload`, o activar la feature `stream` de `reqwest`. |
| R4 | M | Instagram: el token largo solo se refresca al caducar; si la app no se abre en 60 días, hay que reconectar. | Refrescar en segundo plano los tokens de Instagram con más de 7 días. |
| R5 | B | La tabla `jobs` (0001) y `api_cache` siguen sin usarse. | Dejarlas (no molestan) o quitarlas en una migración cuando haya versión pública. |
| R6 | B | `retention_curve` usa `OffsetDateTime::now_utc()` en lugar del `Clock` inyectado. | Pasar la fecha de fin desde el command. |

## Rendimiento

| # | Prio | Hallazgo | Propuesta |
|---|---|---|---|
| P1 | **A** | El bundle del frontend pasa de 500 kB (Vite avisa); todas las pantallas se cargan al arrancar. | `React.lazy` por ruta (editor de miniaturas, competencia, patrocinios…). |
| P2 | B | `asset_data_url` codifica la imagen completa en base64 en cada carga. | Caché en memoria por sesión en el editor (ya se cachea por escena) o protocolo `asset:` con ámbito limitado. |
| P3 | B | Agrupación de preguntas O(n²) sobre 2.000 preguntas bajo el mutex de la base de datos. | Suficiente hoy (< 50 ms); MinHash si algún día crece. |

## Consistencia y deuda

| # | Prio | Hallazgo | Propuesta |
|---|---|---|---|
| C1 | **A** | `PublishPage` decide si falta el permiso de subida con una heurística (`/upload|publish/`) en lugar de preguntar al backend como hace `UploadForm`. | Usar `uploadScopes(accountId)` por destino. |
| C2 | M | `lock_db` está copiado en 9 archivos de commands. | Un helper común en `commands/mod.rs`. |
| C3 | M | Los entregables de patrocinios no salen en el calendario. | Añadir `EventKind::Deliverable` a la unión del calendario. |
| C4 | B | Algunos `f64` de specta llegan como `number \| null` y el frontend repite `?? 0`. | Aceptable (es cómo specta serializa NaN); centralizar si molesta. |
| C5 | B | No hay tests de los commands de Tauri (solo del core y del frontend con mocks). | Tests de integración con `tauri::test` cuando la CI funcione. |

## Pendiente fuera del código

- Pruebas manuales de la checklist de `PROGRESS.md` (bandeja, notificaciones, Windows, Instagram/TikTok reales, PIN, chat de Twitch).
- R1 de la arquitectura: la CI nunca se ha ejecutado (cuenta de GitHub suspendida).

## Propuesta para el 7b

Aplicar todas las **A** (S1, S2, R1, P1, C1) y las M baratas (C2). El resto queda anotado.
