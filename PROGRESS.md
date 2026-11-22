# PROGRESS

> Estado del proyecto para cualquier sesión nueva (tras `/clear`). Ver `OMNYRA_SPEC.md` sección 9.

**Paso actual:** 2c — Segundo plano y notificaciones (hecho con Opus). Implementado y con tests; bandeja, cerrar-a-bandeja, autostart, notificaciones (recordatorios + alertas) y planificador periódico. Falta prueba manual real (bandeja/notificaciones en GNOME/KDE y Windows). Después: 3a.

## Pasos

- [x] 0.0 — Preparación del entorno (usuario)
- [x] 0a — Arquitectura
- [x] 0b — Base del proyecto
- [x] 0c — CI multiplataforma
- [x] 1a — OAuth de Google y almacenamiento seguro
- [x] 1b — Conector de YouTube y snapshots
- [x] 1c — Mejor hora para publicar
- [x] 1d — Subida de vídeos a YouTube
- [x] 1e — Pantallas del MVP
- [x] 2a — Conector de Twitch
- [x] 2b — Calendario y tablero de ideas
- [x] 2c — Segundo plano y notificaciones
- [x] 3a — Estrategia de autenticación Meta y TikTok
- [x] 3b — Conectores de Instagram y TikTok
- [x] 3c — Publicación multi-red
- [x] 4a — Bandeja de comentarios unificada
- [x] 4b — Clasificación y preguntas repetidas
- [x] 5a — Editor de miniaturas
- [x] 5b — Asistente de textos
- [x] 5c — Detector de momentos para shorts
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

- 2026-11-17 (1d): **cuota de subidas resuelta** (R5 [VERIFICAR]): `videos.insert` ya no cuesta 1.600 unidades; tiene su propio cubo de 100 llamadas/día a 1 unidad (developers.google.com/youtube/v3/determine_quota_cost, consultado 2026-11-17). `quota_ledger` usa cubos `youtube` y `youtube.upload`; `thumbnails.set` y `playlistItems.insert` (50 c/u) van al cubo general. `QuotaGate::spend(bucket, units)` con presupuestos por propósito (`Purpose::Sync` 6.000; `Purpose::Upload` 10.000 + 100).
- 2026-11-17 (1d): subida reanudable escrita a mano (`connectors/youtube/upload.rs`): trozos de 8 MiB, reanudación con `Content-Range: bytes */total`, sesión caducada (404/410) → se empieza de nuevo una vez. La URI de sesión va al llavero (`omnyra/upload/<publication_id>`, H6) mediante `TokenStore::{get,set,delete}_secret`.
- 2026-11-17 (1d): cola = tabla `publications` (migración 0002: `attempts`, `retry_at`, `progress`, `external_id`, `warnings`), no la tabla `jobs` (eso es del 2c). Un trabajador en segundo plano sube de una en una; errores transitorios (red, 5xx, 429) → reintento con backoff 30 s·2^n (máx. 15 min) hasta 5 intentos; el resto → `failed`. Al arrancar, `uploading` → `queued` (se reanuda). Se puede cancelar en marcha (se aborta la transferencia).
- 2026-11-17 (1d): permisos incrementales: `youtube.upload` al subir; `youtube.force-ssl` solo si se elige lista de reproducción (`playlistItems.insert` no lo cubre `youtube.upload`). Comando `authorize_upload`.
- 2026-11-17 (1d): validación = metadatos del archivo + ffprobe (`media/probe.rs`, ruta con `OMNYRA_FFPROBE`) + reglas de YouTube (formatos, 256 GB, 12 h y aviso de 15 min sin verificar, título 100 caracteres, descripción 5.000 bytes, sin `<>`, etiquetas 500, >60 hashtags, miniatura JPG/PNG ≤ 2 MB). Sin ffprobe se avisa y se deja subir. La miniatura o la lista que fallen son avisos, no errores (las miniaturas personalizadas exigen canal verificado por teléfono).
- 2026-11-17 (1d): **cambio sobre H9**: la descarga verificada de FFmpeg en Windows NO se ha hecho en el 1d; sin ffprobe la subida funciona con un aviso. Se hará cuando haga falta FFmpeg de verdad (renderizados del 3c) o en el 7c.
- 2026-11-17 (1d): «hecho para niños» es obligatorio en el formulario (sin valor por defecto). Categoría por defecto: Juegos (20) si existe.

- 2026-11-17 (1e): lecturas para pantallas en `omnyra-core/src/dashboard.rs` (resumen por cuenta y global de un periodo, series combinadas entre cuentas con arrastre del último valor para `followers`, top contenidos). El periodo (30 días por defecto, 1 año, 5 años, todo) es común a todas las pantallas y se recuerda (`localStorage`, solo comodidad).
- 2026-11-17 (1e): Panel global = tarjetas (suscriptores y su cambio, visualizaciones, horas vistas, publicaciones), gráfica combinada, top 5 del periodo y lista de cuentas con su última sincronización; "Sincronizar todo". Vista de cuenta (`/platform/:platform/:id`) = tarjetas detalladas, gráfica, mejor hora y tabla de contenidos con filtro por tipo y orden. Subidas en `/uploads`. Sin miniaturas remotas en la tabla (la CSP no las permite; se descargarán a caché local más adelante).
- 2026-11-17 (1e): interfaz funcional y sin pulir a propósito (ver preferencia del usuario).

- 2026-11-17 (2a): Twitch como **cliente público** con **device code flow** (RFC 8628): es el único flujo que Twitch permite sin client secret (dev.twitch.tv/docs/authentication/getting-tokens-oauth, consultado 2026-11-17). Sin secreto en el binario. Refresh tokens de un solo uso que caducan a los 30 días sin uso (cada refresco guarda el nuevo). `OAuthConfig.device_url` activa el flujo; `AuthKind::DeviceCode`; la UI muestra el código (evento `device-code-issued`) y abre twitch.tv/activate. `scope` como array y errores `{status,message}` de Twitch normalizados en `oauth.rs`.
- 2026-11-17 (2a): permiso pedido: `moderator:read:followers` (solo lectura). Todas las llamadas Helix llevan `Client-Id` (`Connector::api_headers`). Sin cuota diaria (solo límite por minuto): no se apunta en `quota_ledger`.
- 2026-11-17 (2a): datos: seguidores (total diario + histórico a partir de `followed_at` de los seguidores actuales, como `subscribers_gained` diario → la reconstrucción de seguidores existente), VODs (`vod`; los `upload` son `video`) y clips (`clip`) con sus vistas, `stream_id` del VOD en `contents.extra`. Twitch no lista en orden cronológico (clips por vistas): `Connector::lists_newest_first() = false` y la sincronización recorre todas las páginas (tope 200).
- 2026-11-17 (2a): espectadores medios y pico (A2): muestreo cada 2 min de `/helix/streams` mientras la app está abierta → `live_sessions`/`live_samples` (migración 0003). Horas emitidas y nº de directos = VODs de tipo archive del periodo + sesiones observadas sin VOD (emparejadas por `stream_id`). Sin EventSub de momento (la bandeja y el segundo plano son del 2c).
- 2026-11-17 (2a): vista propia de Twitch (`src/platforms/twitch/TwitchView.tsx`, registrada como `AnalyticsView`): seguidores, directos, horas, espectadores medios/pico (observados), gráfica de seguidores, tabla de directos vistos y tabla de VODs/clips. Sin botón de subida (`upload: none`). Cabecera de cuenta extraída a `AccountHeader`.

- 2026-11-17 (2a): la API de Twitch solo expone los VODs que Twitch aún guarda (7/14/60 días según el tipo de cuenta, y solo con "Almacenar emisiones anteriores" activado). Omnyra conserva en `contents` los VODs que ha visto aunque Twitch los borre, así que el historial de directos se completa desde que se usa la app; lo anterior no es recuperable.

- 2026-11-17 (2b): migración 0004: `ideas`, `idea_files`, `reminders` y `publications.idea_id` (decisión del 0b). `ideas.source_cluster_id` va sin FK porque `question_clusters` llega en el 4b. Lógica en `omnyra-core/src/planning.rs`.
- 2026-11-17 (2b): calendario = unión de `contents.published_at` (no se puede mover), `publications.scheduled_at`, `reminders.at` e `ideas.due_at`; se consulta por instantes `[from, to)` que la UI calcula en hora local (comparación con `julianday` por las distintas precisiones). Mover conserva la hora local del día (vista de mes) o fija la hora (vista de semana). Filtro por red.
- 2026-11-17 (2b): Kanban con posiciones fraccionarias (solo se escribe la tarjeta movida; si el hueco baja de 1e-9 se renumera la columna). Archivos de las ideas por referencia (no se copian ni se borran nunca del disco); se abren con `open_idea_file`, que solo abre archivos adjuntos (el frontend no tiene un "abrir cualquier ruta").
- 2026-11-17 (2b): arrastrar y soltar con HTML5 nativo; `dragDropEnabled: false` en la ventana (si no, Tauri intercepta los arrastres en Windows). Omnyra no usa soltar archivos del sistema.

- 2026-11-18 (2c): **decisión de alcance**: NO se reconstruyen los bucles existentes (sync de arranque, worker de subidas, muestreo de directos) sobre la tabla `jobs`. Se añade un **planificador periódico** ligero (`src-tauri/src/background.rs`) que reutiliza la lógica de obsolescencia ya probada (`sync::due_accounts`, `AUTO_SYNC_AFTER` = 6 h). La tabla `jobs` (ya creada en 0001) queda sin usar hasta que haya trabajos que necesiten persistir entre reinicios (render/publish del 3c). Las tablas `alerts`, `settings` y `reminders.notified_at` ya existían desde 0001; 2c no añade migración.
- 2026-11-18 (2c): bandeja (`src-tauri/src/tray.rs`, feature `tauri` `tray-icon`+`image-png`): menú Abrir/Sincronizar todo/Salir, clic izquierdo muestra la ventana, etiquetas según `locale`. "Seguir en segundo plano al cerrar" (`close_to_tray`): el `CloseRequested` oculta la ventana en vez de salir (`install_close_handler` en `lib.rs`); los planificadores siguen. Con `single-instance`, un segundo lanzamiento vuelve a mostrar la ventana.
- 2026-11-18 (2c): ajustes = repo key/value `settings` (`db/repo/settings.rs`, claves tipadas en `keys`); defaults: `close_to_tray=false`, `autostart=false`, `background_sync=true`, `notifications=true`, `locale="es"`. Comandos `get_settings`/`update_settings`/`set_locale`. Autostart vía `tauri-plugin-autostart` (el plugin es la fuente de verdad del SO; se guarda el estado logrado para la UI). Pantalla: `PreferencesSection` en Ajustes.
- 2026-11-18 (2c): notificaciones (`tauri-plugin-notification`) disparadas desde Rust, no desde JS → mensajes localizados a mano en `src-tauri/src/notify.rs` (ES/EN) porque el OS las pinta fuera del webview; el frontend espeja su idioma con `set_locale` (en `setLang` y al arrancar). Permiso `notification:default` en capabilities.
- 2026-11-18 (2c): alertas (`omnyra-core/src/alerts.rs`, probado): hitos de seguidores (`{1,5}·10^k`, k≥2) comparando los dos últimos días (una sola jornada → nada, para no disparar por la reconstrucción histórica) y **rendimiento** por z-score (|z|≥2, muestra ≥8) de `views_7d` (ya normalizado por antigüedad) frente a los contenidos del mismo tipo; solo para contenidos publicados en los últimos 21 días. Dedup: hito por (cuenta, umbral), rendimiento 1 por contenido. `evaluate_account` se llama tras cada sync (manual y en segundo plano) desde `sync::run`; emite `AlertsChanged` y notifica las nuevas. La caída de engagement (media móvil de 4 semanas) se aplaza: hacen falta semanas de datos diarios continuos que la app apenas empieza a recopilar.
- 2026-11-18 (2c): recordatorios → notificación: bucle cada 60 s (`spawn_reminder_notifier`) que busca `reminders` con `at <= now` y `notified_at IS NULL`, notifica y marca. El planificador de snapshots corre cada 30 min pero solo sincroniza cuentas realmente obsoletas. La publicación programada automática real (A5) es del 3c; de momento solo se notifica el recordatorio.
- 2026-11-22: el usuario pidió terminar todos los pasos restantes en una sola sesión con `claude-opus-5-5` (sin cambiar a Fable en 3a/7a ni parar entre fases). Cada paso lleva su commit; lo que depende de cuentas o credenciales del usuario queda en "Problemas pendientes" y en la checklist manual.
- 2026-11-22 (3a): `docs/AUTH_STRATEGY.md`: **token broker** sin estado en Cloudflare Workers (`broker/`, con tests `node --test`) que guarda los secretos de Meta y TikTok, hace el intercambio y el refresco, y para Instagram sirve el `redirect_uri` HTTPS y reenvía al loopback (puerto dentro de `state`, siempre `127.0.0.1`). La app nunca lleva esos secretos.
- 2026-11-22 (3b): `OAuthConfig` gana `client_id_param` (TikTok `client_key`), `scope_separator` (`,`), `pkce_hex` (TikTok desktop, [VERIFICAR]) y `redirect_broker`. Conectores `instagram` (perfil, `reach` y seguidores nuevos de los últimos 30 días, Reels/posts con vistas/likes/comentarios, publicación de Reels con `rupload`, comentarios) y `tiktok` (perfil y totales, vídeos con vistas/likes/comentarios/compartidos, Direct Post por trozos con caída a `SELF_ONLY`). Reglas comunes de vídeo corto en `connectors/rules.rs` (el pie = título + descripción + etiquetas como hashtags). Vista compartida `src/platforms/shortform/ShortFormView.tsx`. El formulario de subida oculta los campos de YouTube en estas redes. Sin probar contra las APIs reales (faltan las apps).
- 2026-11-22 (3c): pipeline FFmpeg en `omnyra-core/src/media/ffmpeg.rs` (argumentos puros y probados + `render` con progreso `-progress pipe:1`; test real con FFmpeg marcado `#[ignore]`): 16:9 / 9:16 / 1:1 con recorte, bandas o fondo difuminado, sin escalar por encima de 1080 en el lado corto; recorte de inicio/fin y duración máxima de la red; tope de bitrate para caber en el límite de tamaño (si no, CRF 23). Presets: Instagram y TikTok en vertical. Migración 0005: `publications.render_spec`; estado `rendering` → trabajador FFmpeg (`src-tauri/src/renders.rs`, de uno en uno) → archivo gestionado en `app_data_dir/renders/` → `queued`. La subida usa la versión si existe. Programación local real (A5): `scheduled_at` en la cola, el trabajador de subidas espera a esa hora. Asistente en `/publish` (texto común o propio por red, formato, recorte, programación) y "Preparar y descargar" (exporta a la ruta elegida y copia el texto). Los tests del adaptador los escribí en la sesión principal (no se delegó a `test-writer`).
- 2026-11-22 (4a): migración 0006 (`comments`, `question_clusters` con `idea_id`; sin FTS: el buscador usa `LIKE` escapado, suficiente para el volumen de un creador). `Comment` unificado (padre, contenido, autor, propio, likes). Lógica en `omnyra-core/src/community/`: YouTube lee todo el canal con `commentThreads.list?allThreadsRelatedToChannelId` (1 unidad/100 hilos, basta `youtube.readonly`; responder = `comments.insert`, 50 unidades, `youtube.force-ssl` pedido al responder por primera vez); Instagram, la primera página de los 20 contenidos más recientes (`instagram_business_manage_comments`). Twitch y TikTok no tienen comentarios por API. Las respuestas propias marcan el hilo como `replied`; cada respuesta va al `audit_log` (`reply`). Los comentarios se sincronizan tras cada sync de cuenta (sin bloquearla si falta el permiso). Bandeja en `/community` con filtros por red, cuenta, estado, categoría y texto.
- 2026-11-22 (4b): clasificación por reglas ES/EN en `community/classify.rs` (trait `Classifier` para un modelo futuro): orden spam > pregunta > negativo > positivo > neutro; texto normalizado (minúsculas, sin acentos, letras repetidas acortadas); spam = 2 señales o 1 señal + enlace; palabras completas para términos sueltos. La categoría manual (`category_source = 'manual'`) nunca se sobrescribe. Preguntas repetidas en `community/cluster.rs`: palabras de contenido (sin palabras vacías ni plural) → 3-shingles de caracteres → Jaccard ≥ 0,4, agrupación de enlace simple, exacta O(n²) sobre las 2.000 preguntas abiertas más recientes (sin MinHash/LSH: no hace falta a este volumen). Los IDs de grupo se conservan entre reconstrucciones para que el vínculo con la idea no se pierda. "Enviar al tablero de ideas" crea una idea con las preguntas en las notas (`ideas.source_cluster_id`).
- 2026-11-22 (5a): editor de miniaturas con Canvas 2D a mano (sin `konva`: no hacía falta otra dependencia para capas, arrastrar y exportar). Escena JSON (`src/features/creation/thumbnails/scene.ts`, probada): fondo (color o imagen), imágenes de la biblioteca (voltear), texto con contorno y sombra, rectángulos/elipses, stickers emoji; capas con orden, visibilidad, duplicar; cambio de tamaño de lienzo conservando posiciones; exportación "cover" a YouTube 1280×720 (JPEG, <2 MB), vertical 1080×1920, cuadrado 1080×1080 y 1920×1080. Migración 0007 (`assets`, `thumbnail_templates`, `text_templates`). Biblioteca: las imágenes se **copian** a `app_data_dir/files/` (archivo gestionado) y llegan al webview como data URL (la CSP no cambia). "Usar en una subida" guarda en `app_data_dir/thumbnails/` y abre `/uploads?thumbnail=…`.
- 2026-11-22 (5b): asistente de textos. Plantillas en TS (`src/features/creation/texts/templates.ts`, probadas): variables `{{tema}}`, `{{juego}}`, `{{serie}}`, `{{episodio}}`, `{{canal}}`, `{{fecha}}`, `{{hashtags}}` (en mayúsculas = valor en mayúsculas), limpieza de separadores huérfanos, plantillas de serie por red + las del usuario (`text_templates`), hashtags sin acentos/CamelCase dentro del rango recomendado de cada red, aviso de longitud. IA opcional en Rust (`omnyra-core/src/assistant.rs`): API de Mensajes de Claude por HTTP (no hay SDK oficial de Rust), `anthropic-version: 2026-10-21`, modelo `claude-opus-5-5`, `effort: low` (tarea corta y sencilla), salida estructurada con esquema JSON, `fallbacks: "default"` (beta `server-side-fallback-2026-07-01`) y `stop_reason: refusal` → `LLM_REFUSED`. La clave va al llavero (`omnyra/llm/anthropic`, H12) y la UI muestra exactamente qué se envía antes de enviarlo. Errores nuevos: `LLM_KEY_MISSING`, `LLM_KEY_INVALID`, `LLM_REFUSED` (y `LOCKED` para el 6c).
- 2026-11-22 (5c): detector de momentos (`omnyra-core/src/moments.rs`, probado). YouTube: curva de retención de Analytics (`audienceWatchRatio` por `elapsedVideoTimeRatio`, filtro `audienceType==ORGANIC`), guardada en `content_series` (migración 0008) y renovada cada 7 días; picos = residuo sobre la mediana móvil (ventana 9) ≥ 3× el ruido (MAD), ignorando el primer 3 %. Twitch: Omnyra lee el chat público por IRC anónimo (`justinfan`, puerto 6667 en claro: es público y no se envía ninguna credencial) mientras la cuenta está en directo y guarda mensajes por cubo de 10 s en `live_chat`; picos con z robusto (mediana/MAD) ≥ 3, ventana desplazada 15 s hacia atrás (el chat reacciona tarde). Ventanas de 15–60 s fusionadas, máximo 8. El recorte en vertical usa el pipeline de FFmpeg del 3c sobre el archivo local que elige el usuario (no se puede descargar por API) y abre `/publish?file=…`.
- 2026-11-18 (2c): campana de alertas en una cabecera nueva del `Layout` (`src/features/alerts/AlertsBell.tsx`): contador de no leídas, panel con las últimas 30, marcar leída al pulsar y "marcar todo como leído".

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
- 2026-11-18 (2c): la bandeja en Linux necesita `libayatana-appindicator3` (o `libappindicator3`) en tiempo de ejecución y un servidor StatusNotifierItem (H11). Añadir la dependencia al `.deb` (`Depends`) y al `PKGBUILD` en el 7c; en Wayland/Hyprland del usuario existe (`org.kde.StatusNotifierWatcher` de quickshell). Para `cargo tauri dev` en local puede hacer falta instalar el paquete del sistema.
- 2026-11-18 (2c): autostart no reconciliado al arrancar (el SO mantiene su propio estado); si el usuario desactiva el autostart por fuera, el ajuste mostrado puede quedar desincronizado hasta que lo togglee. Aceptable.

## Checklist manual (pruebas reales por sistema)

- [ ] 5a: importar las 3 skins de gh0stymc (normal, Halloween, Navidad) en Creación → Biblioteca, montar una miniatura, guardarla como plantilla y usarla en una subida
- [ ] 5c: buscar momentos en un vídeo largo de gh0stymc con retención y recortar uno; abrir la app durante un directo de Twitch y comprobar que se graba el chat (`live_chat`)
- [ ] 3b: desplegar el broker y conectar Instagram (cuenta profesional + tester) y TikTok (sandbox); sincronizar; publicar un Reel y un TikTok privado. Revisar los [VERIFICAR] de AUTH_STRATEGY.md (PKCE hex, versión de la Graph API, límites de Reels).

- [x] Llavero (Secret Service) en Linux — `cargo test -p omnyra-core -- --ignored keyring` pasa con gnome-keyring (2026-11-16)
- [x] Flujo OAuth real con Google en Linux (conectar gh0stymc, reiniciar la app, desconectar y comprobar que desaparece de myaccount.google.com/connections) (2026-11-17)
- [ ] Flujo OAuth real en Windows
- [x] Sincronización real de gh0stymc (1b): 100 contenidos (94 vídeos, 6 Shorts, todos clasificados), seguidores reconstruidos desde 2020-03-30, 9 unidades de cuota en el día (2026-11-17)
- [x] Subida real a gh0stymc (1d): permiso incremental, vídeo corto con miniatura, cancelar a mitad, reanudar tras cerrar la app
- [x] Twitch real (2a): conexión con código y sincronización (1 VOD, 41 clips) (2026-11-17)
- [ ] Twitch: abrir la app durante un directo y ver la sesión con espectadores
- [ ] 2c: bandeja del sistema (mostrar/ocultar, menú Sincronizar/Salir) en Linux (GNOME y KDE) y Windows
- [ ] 2c: "seguir en segundo plano al cerrar" (la ventana se oculta, los planificadores siguen)
- [ ] 2c: autostart (abrir al iniciar sesión) en Linux y Windows
- [ ] 2c: notificaciones del sistema (recordatorio vencido, hito, rendimiento) en GNOME/KDE y Windows
- [ ] Llavero (Credential Manager) en Windows
- [ ] Rutas de `app_data_dir` en Windows
- [ ] Instalar y abrir el `.msi`/`.exe` en Windows (VM)
- [ ] Instalar y abrir el `.deb` en Debian/Ubuntu
- [ ] Instalar y abrir el paquete de Arch (`makepkg` ya compila en local, 2026-11-16)
