# PROGRESS

> Estado del proyecto para cualquier sesión nueva (tras `/clear`). Ver `OMNYRA_SPEC.md` sección 9.

**Paso actual:** 1a — OAuth de Google y almacenamiento seguro (`claude-opus-5-5`). Fase 0 hecha; falta la confirmación del usuario para empezar la Fase 1.

## Pasos

- [x] 0.0 — Preparación del entorno (usuario)
- [x] 0a — Arquitectura
- [x] 0b — Base del proyecto
- [x] 0c — CI multiplataforma
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
- 2026-11-16 (R1): CI = GitHub Actions (`.github/workflows/ci.yml`), fino sobre `scripts/ci/check.sh` y `scripts/ci/bundle.sh` para poder portarlo.
- 2026-11-16 (H9): FFmpeg en Windows no va en el instalador: se descarga en el primer uso con verificación SHA-256 y ruta configurable (se implementa en 1d). En Linux es dependencia del paquete (`.deb` y `PKGBUILD`); en AppImage se usa el del sistema.
- 2026-11-16: los `.deb`/AppImage se compilan en Ubuntu 22.04 (glibc más antigua → más distros compatibles). `.rpm` aplazado: el nombre del paquete FFmpeg cambia según el repo de Fedora.
- 2026-11-16: las acciones de GitHub van fijadas por SHA de commit.

## Problemas pendientes

- R1: la CI no se ha ejecutado nunca: la cuenta de GitHub sigue suspendida y el repo no tiene remoto. Revisar la primera ejecución al subirlo (no se pudo pasar `actionlint` en local).
- `packaging/arch/PKGBUILD`: `_owner=OWNER` y `sha256sums=('SKIP')` se rellenan en el 7c, cuando exista el repo público y el primer tag.
- Idiomas de los instaladores de Windows (WiX/NSIS en español), `publisher`/maintainer del `.deb` y `homepage`: pulir en el 7c.
- Cuando el 1a añada `keyring`, revisar si el `.deb`/`PKGBUILD` necesitan `libsecret`/`libdbus` como dependencia.
- Falta `rustup`/toolchain fijada: se usa el Rust del sistema (1.100).

## Checklist manual (pruebas reales por sistema)

- [ ] Llavero (Secret Service) en Linux
- [ ] Llavero (Credential Manager) en Windows
- [ ] Rutas de `app_data_dir` en Windows
- [ ] Instalar y abrir el `.msi`/`.exe` en Windows (VM)
- [ ] Instalar y abrir el `.deb` en Debian/Ubuntu
- [ ] Instalar y abrir el paquete de Arch (`makepkg` ya compila en local, 2026-11-16)
