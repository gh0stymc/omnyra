# PROGRESS

> Estado del proyecto para cualquier sesión nueva (tras `/clear`). Ver `OMNYRA_SPEC.md` sección 9.

**Paso actual:** 0c — CI multiplataforma (`claude-opus-5-5`)

## Pasos
- [x] 0.0 — Preparación del entorno (usuario)
- [x] 0a — Arquitectura
- [x] 0b — Base del proyecto
- [ ] 0c — CI multiplataforma
- [ ] 1a — OAuth de Google y almacenamiento seguro
- [ ] 1b — Conector de YouTube y snapshots
- [ ] 1c — Mejor hora para publicar
- [ ] 1d — Subida de vídeos a YouTube
- [ ] 1e — Pantallas del MVP
- [ ] 2a — Conector de Twitch
- [ ] 2b — Calendario y tablero de ideas
- [ ] 2c — Segundo plano y notificaciones
- [ ] 3a — Estrategia de autenticación Meta y TikTok
- [ ] 3b — Conectores de Instagram y TikTok
- [ ] 3c — Publicación multi-red
- [ ] 4a — Bandeja de comentarios unificada
- [ ] 4b — Clasificación y preguntas repetidas
- [ ] 5a — Editor de miniaturas
- [ ] 5b — Asistente de textos
- [ ] 5c — Detector de momentos para shorts
- [ ] 6a — Patrocinios y media kit
- [ ] 6b — Competencia y tendencias
- [ ] 6c — Módulo de seguridad
- [ ] 7a — Revisión global
- [ ] 7b — Aplicar mejoras
- [ ] 7c — Distribución
- [ ] 7d — Documentación final

## Decisiones
- 2026-11-16: ARCHITECTURE.md aprobado tal cual. El paso 0a se redactó con Opus (no Fable).
- 2026-11-16: `tauri-specta` aprobado (tipos Rust→TS en `src/lib/bindings.ts`; se regenera con `cargo test -p omnyra`).
- 2026-11-16: gestor de paquetes JS = npm.
- 2026-11-16: `rusqlite` fijado a 0.41 (es lo que soporta `rusqlite_migration` 2.7; 0.43 entra en conflicto por `links = "sqlite3"`).
- 2026-11-16: la migración 0001 no incluye `publications.idea_id`; se añade con la migración que crea `ideas` (2b).

## Problemas pendientes
- R1: cuenta de GitHub suspendida: decidir proveedor de CI en el 0c (GitHub / Codeberg / GitLab).
- Falta `rustup`/toolchain fijada: se usa el Rust del sistema (1.100).

## Checklist manual (pruebas reales por sistema)
- [ ] Llavero (Secret Service) en Linux
- [ ] Llavero (Credential Manager) en Windows
- [ ] Rutas de `app_data_dir` en Windows
