# PROGRESS

> Estado del proyecto para cualquier sesión nueva (tras `/clear`). Ver `OMNYRA_SPEC.md` sección 9.

**Paso actual:** 1d — Subida de vídeos a YouTube (`claude-opus-5-5`). El 1c está hecho y probado con gh0stymc.

## Pasos

- [x] 0.0 — Preparación del entorno (usuario)
- [x] 0a — Arquitectura
- [x] 0b — Base del proyecto
- [x] 0c — CI multiplataforma
- [x] 1a — OAuth de Google y almacenamiento seguro
- [x] 1b — Conector de YouTube y snapshots
- [x] 1c — Mejor hora para publicar
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
- 2026-11-16 (1a): OAuth escrito a mano sobre `reqwest` 0.13 (sin el crate `oauth2`): ~150 líneas, una sola pila HTTP y control total de lo que llega a errores y logs. PKCE S256, `state` comparado en tiempo constante, loopback `http://127.0.0.1:<puerto>/` que ignora peticiones con `state` incorrecto (un proceso local no puede abortar el flujo) y no refleja nada de la petición.
- 2026-11-16 (1a): llavero = `keyring` 4 (Windows Credential Manager + Secret Service vía `zbus`, Rust puro: no enlaza `libsecret` ni `libdbus`). Entradas `omnyra/<account_uuid>`; llamadas siempre en `spawn_blocking`. Sin llavero → `KEYRING_UNAVAILABLE`, nunca texto plano (D4).
- 2026-11-16 (1a): client ID/secret de Google se incrustan en compilación (`src-tauri/build.rs` lee `.env`; las variables de entorno tienen prioridad, para la CI). Sin client ID, YouTube aparece como "no configurado".
- 2026-11-16 (1a): al conectar solo se piden `youtube.readonly` + `yt-analytics.readonly`, con `prompt=select_account consent` (siempre hay refresh token y se puede elegir canal/cuenta de marca) e `include_granted_scopes`. `connect(..., extra_scopes)` ya admite la autorización incremental para `youtube.upload` (1d) y `youtube.force-ssl` (4a). Si el usuario desmarca un permiso → `SCOPES_DENIED` y no se guarda nada.
- 2026-11-16 (1a): tras un fallo después del intercambio, los tokens se descartan en memoria; solo se revoca el grant si la cuenta no tiene canal (`NO_CHANNEL`), para no romper una cuenta ya conectada con el mismo grant.
- 2026-11-16 (1a): desconectar = revocar (best effort) → borrar del llavero → `status = disconnected` (los datos se conservan). Si no se puede revocar (sin red), se borra igualmente en local y la UI enlaza a `myaccount.google.com/connections`.
- 2026-11-16 (1a): refresco 60 s antes de caducar, serializado por cuenta; un 401 refresca una vez y reintenta. `invalid_grant` → `TOKEN_EXPIRED` → la cuenta pasa a `needs_reauth` y se emite el evento `account-changed`.
- 2026-11-17: logging con `tracing` (core) + `tracing-subscriber` a stderr en la app; filtro con `RUST_LOG` (por defecto `warn`). Nunca se registran tokens.
- 2026-11-16 (1a): el `audit_log` (cadena de hashes SHA-256) empieza a usarse ya: `account.connect` y `account.disconnect`.

- 2026-11-17 (1b): sincronización = `channels.list` (perfil y totales) → `playlistItems` de la lista "uploads" (`UC…` → `UU…`, sin `search.list`) con parada en el primer vídeo ya conocido → `videos.list` en lotes de 50 (refresca las estadísticas de todos) → detección de Shorts → Analytics API (`dimensions=day`, tramos de 365 días; la primera vez desde la creación del canal y luego desde el último día histórico − 3) → reconstrucción de `followers` hacia atrás desde el total real. Lógica en `omnyra-core/src/sync.rs`; cada fase en su transacción.
- 2026-11-17 (1b): métricas diarias (`historic`): `views`, `watch_time_s`, `avg_view_duration_s`, `subscribers_gained`, `subscribers_lost`, `likes`, `comments`, `shares`. Totales (`current`): `followers`, `views_total`, `contents_total`. Por contenido: `views`, `likes`, `comments`.
- 2026-11-17 (1b): Shorts = `HEAD https://www.youtube.com/shorts/<id>` sin token y con la única cookie `SOCS=CAI` ("rechazar todo" del aviso de cookies de la UE; sin ella, desde España todo redirige a consent.youtube.com y no se clasificaba nada): 200 → `short`, 303 a `/watch` → `video`, otra cosa → sin clasificar (se reintenta). Una vez por vídeo (`extra.kind_checked`). Elegido por el usuario; no es API oficial. `stream` si tiene `liveStreamingDetails`.
- 2026-11-17 (1b): cuota en `quota_ledger` por día del Pacífico (regla DST de EE. UU. a mano). La sincronización puede gastar el 60 % (6.000 unidades); sin presupuesto se salta la Data API y se sigue con Analytics (cuota aparte). Reintentos con backoff y jitter (máx. 3) en 429/5xx/red; `Retry-After` > 60 s no se espera.
- 2026-11-17 (1b): sin caché ETag por ahora: el listado incremental y la clasificación única ya evitan lo repetido, y no está claro que un 304 no gaste cuota. `api_cache` queda sin usar.
- 2026-11-17 (1b): sincronización automática al arrancar para las cuentas activas con `last_sync_at` de hace más de 6 h, en serie. `SyncLocks` impide dos a la vez sobre la misma cuenta (`SYNC_IN_PROGRESS`). Evento `sync-status`. Sin cola `jobs` hasta el 2c.

- 2026-11-17 (1c): algoritmo documentado en `omnyra-core/src/best_time.rs`. Muestra = un formato (vídeos o Shorts; directos fuera) con `views_7d`; `y = ln(1+views_7d)`, peso `0.5^(edad/180)`, score = y − media ponderada; cada contenido suma a su (día, hora local) y la mitad a la hora anterior y siguiente; casilla = Σws/(Σw+1); < 15 contenidos → "datos insuficientes"; casillas con peso < 0,75 → "pocos datos" y nunca se recomiendan. Top 3 sin horas consecutivas.
- 2026-11-17 (1c): `views_7d` = visitas de los 7 primeros días por Analytics (`filters=video==ID`, una petición por vídeo, cuota de Analytics), guardado como `content_snapshots` (`day` = día de publicación, `historic`). Se pide una vez por contenido con al menos 10 días. El día de publicación es el UTC (YouTube Analytics usa días del Pacífico: error ≤ 1 día, aceptable).
- 2026-11-17 (1c): por defecto solo vídeos públicos; interruptor "incluir no listados" con aviso de que es orientativo (decisión del usuario: gh0stymc tiene 96 de 100 no listados).
- 2026-11-17 (1c): zona horaria = la del sistema (`Intl` en el frontend, validada con `jiff` en Rust). El ajuste manual de zona (H8) queda para cuando haya pantalla de ajustes generales.
- 2026-11-17 (1c): los tests del algoritmo los escribió un subagente con las instrucciones de `test-writer` (los agentes de `.claude/agents/` no se cargan si Claude Code se abre fuera de la carpeta del proyecto).

- 2026-11-17: el usuario prefiere dejar la estética para el final (pedirá un repaso visual cuando esté todo); hasta entonces, interfaz funcional y mínima.

## Problemas pendientes

- R1: la CI no se ha ejecutado nunca: la cuenta de GitHub sigue suspendida y el repo no tiene remoto. Revisar la primera ejecución al subirlo (no se pudo pasar `actionlint` en local).
- `packaging/arch/PKGBUILD`: `_owner=OWNER` y `sha256sums=('SKIP')` se rellenan en el 7c, cuando exista el repo público y el primer tag.
- Idiomas de los instaladores de Windows (WiX/NSIS en español), `publisher`/maintainer del `.deb` y `homepage`: pulir en el 7c.
- `keyring` no enlaza `libsecret`/`libdbus` (usa `zbus`), así que no hacen falta como dependencia. El `PKGBUILD` declara `org.freedesktop.secrets` como `optdepends`; en el `.deb` no hay un paquete virtual equivalente: decidir en el 7c si se añade `Recommends: gnome-keyring`. `libsecret-1-dev` sobra ya en la CI (inofensivo).
- `reqwest` 0.13 usa rustls con `aws-lc-rs`: en la primera ejecución de la CI en Windows, comprobar que `aws-lc-sys` compila sin NASM/CMake (si no, pasar a `ring`).
- YouTube: `fetch_channel_metrics` y `list_contents` devuelven `NOT_SUPPORTED` hasta el 1b.
- Si el `keyring` falla al iniciarse (Secret Service caído al arrancar), el error se mantiene hasta reiniciar la app (el crate inicializa el almacén una sola vez).
- Revocación (2026-11-17): el primer intento real de desconexión devolvió `revoked: false` (aviso de "¿sin conexión?") con la red funcionando; los dos siguientes, incluido conectar → reiniciar → desconectar, fueron bien. No se sabe la causa: ahora `oauth::revoke` registra el estado HTTP y el código de error (`RUST_LOG=omnyra_core=debug`). Si se repite, mirar ese log.
- [VERIFICAR] `subscriberCount` con `mine=true`: ¿viene redondeado (3 cifras significativas)? Afecta al ancla de la reconstrucción de seguidores.
- 2026-11-17 (1b): las gráficas tienen selector de periodo (30 días por defecto, 1 año, 5 años, todo); el filtro es en el frontend, el histórico completo sigue en la base de datos.
- [VERIFICAR] los estrenos (premieres) también tienen `liveStreamingDetails` y ahora cuentan como `stream`.
- [VERIFICAR] Shorts privados u ocultos: la comprobación sin cookies puede dar 404 y quedarse sin clasificar.
- Falta `rustup`/toolchain fijada: se usa el Rust del sistema (1.100).

## Checklist manual (pruebas reales por sistema)

- [x] Llavero (Secret Service) en Linux — `cargo test -p omnyra-core -- --ignored keyring` pasa con gnome-keyring (2026-11-16)
- [x] Flujo OAuth real con Google en Linux (conectar gh0stymc, reiniciar la app, desconectar y comprobar que desaparece de myaccount.google.com/connections) (2026-11-17)
- [ ] Flujo OAuth real en Windows
- [x] Sincronización real de gh0stymc (1b): 100 contenidos (94 vídeos, 6 Shorts, todos clasificados), seguidores reconstruidos desde 2020-03-30, 9 unidades de cuota en el día (2026-11-17)
- [ ] Llavero (Credential Manager) en Windows
- [ ] Rutas de `app_data_dir` en Windows
- [ ] Instalar y abrir el `.msi`/`.exe` en Windows (VM)
- [ ] Instalar y abrir el `.deb` en Debian/Ubuntu
- [ ] Instalar y abrir el paquete de Arch (`makepkg` ya compila en local, 2026-11-16)
