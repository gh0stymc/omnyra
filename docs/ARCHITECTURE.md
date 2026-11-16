# Omnyra — Arquitectura (paso 0a)

> Estado: **BORRADOR, pendiente de aprobación** · 2026-11-16 · redactado con `claude-opus-5-5` (el usuario eligió Opus en lugar de Fable para este paso).
>
> Lo que lleva **[DECIDIR]** necesita tu respuesta antes del paso 0b. Lo que lleva **[VERIFICAR]** depende de la documentación actual de cada API y se comprobará al implementar el conector correspondiente, como pide la sección 7 del spec.

---

## 1. Revisión del spec: contradicciones, riesgos y huecos

### 1.1 Bloqueantes o de impacto alto

| # | Tema | Problema | Propuesta |
|---|---|---|---|
| R1 | **Cuenta de GitHub suspendida** | Los pasos 0c (CI con GitHub Actions), 7c (releases, updater, `PKGBUILD` que descarga de GitHub) y la creación del repo dependen de GitHub. | El trabajo local no se bloquea: el repo se crea con `git init` en local y se añade el remoto más tarde. Para el 0c, si la cuenta sigue suspendida, hay dos alternativas: **Codeberg (Forgejo Actions)** o **GitLab CI**. La pipeline se escribirá de forma que solo sea un envoltorio fino sobre scripts (`scripts/ci/*.sh` / `.ps1`) para que cambiar de proveedor cueste poco. **[DECIDIR]** en el 0c. |
| R2 | **Google OAuth en modo "Testing"** | Con la pantalla de consentimiento en *Testing* y tipo *External*, los **refresh tokens caducan a los 7 días**. Los snapshots en segundo plano dejarían de funcionar cada semana. | Durante el desarrollo se acepta (con un aviso claro de "reconecta tu cuenta"). Para el uso real: pasar la app a **"In production" sin verificar**. Funciona para uno mismo (aparece la pantalla de "app no verificada" y hay un límite de 100 usuarios). La verificación de Google solo hará falta si se distribuye a terceros. |
| R3 | **"No meter secretos en el binario" frente a Google Desktop** | Los clientes OAuth de tipo *Desktop app* de Google traen un `client_secret` que el endpoint de token exige incluso con PKCE. Google lo documenta como **no confidencial** para apps instaladas. | Se trata como dato público: va en `.env` en desarrollo y se incrusta en la compilación. Se documenta para que no parezca una fuga. Para Meta y TikTok, cuyos secretos sí son confidenciales, se mantiene la regla del spec (paso 3a). |
| R4 | **Subida de YouTube sin auditoría** | Los proyectos de API sin auditar suben los vídeos **bloqueados en privado**. Eso afecta también a `publishAt` (programar), porque el vídeo no podrá pasar a público. | Se muestra un aviso en la UI (ya previsto en el 1d). Se añade un **[DECIDIR]**: ¿pedir la auditoría de cumplimiento de YouTube API cuando el MVP funcione? Es gratuita, pero lenta. |
| R5 | **Cuota de YouTube** | 10.000 unidades/día por proyecto. Una subida cuesta muchas unidades (históricamente 1.600) **[VERIFICAR]**. `search.list` cuesta 100, así que la competencia, la suplantación y las tendencias basadas en búsquedas se comen la cuota muy deprisa. | Habrá un **registro de cuota** en SQLite (tabla `quota_ledger`, con reinicio a medianoche hora del Pacífico), presupuestos por funcionalidad (por ejemplo, reservar el 40 % para subidas) y degradación ordenada. Se usarán ETags y caché. |

### 1.2 Funcionalidades que la API probablemente no permite tal como están descritas

| # | Funcionalidad del spec | Realidad (según lo que sé, **[VERIFICAR]**) | Alternativa |
|---|---|---|---|
| A1 | YouTube: **CTR de miniaturas** | La YouTube Analytics API no expone impresiones ni CTR en las consultas directas. Puede que esté en los informes masivos de la **Reporting API**. | Intentarlo con la Reporting API (jobs diarios); si no está, se quita la métrica y se explica en la UI. |
| A2 | Twitch: **espectadores medios y pico, horas emitidas** | Helix solo da el número de espectadores **en el momento** de la consulta. No existe ningún histórico de analíticas. | **Muestrear durante el directo**: cada 1–2 minutos mientras la cuenta esté en directo (gracias a la bandeja del sistema), con avisos por EventSub `stream.online`/`offline`. Las métricas solo existen para los directos que Omnyra haya visto. Para el pasado solo se pueden usar la duración de los VOD y poco más. |
| A3 | **Detector de momentos por chat de Twitch** | No hay API oficial para recuperar el chat de un VOD. | Grabar la actividad del chat (IRC/EventSub, en cubos de 10 s) **mientras Omnyra esté abierto durante el directo**. Para directos antiguos, solo el detector por retención de YouTube (si se subió allí). Además, el VOD no se descarga por API: el usuario aporta el archivo local. |
| A4 | TikTok: **tiempo medio de visualización** y **comentarios** | La Display API solo da visualizaciones, likes, comentarios (número) y compartidos de cada vídeo. Ni el tiempo medio ni la lectura o respuesta de comentarios están disponibles para apps normales. | TikTok queda con `comments: false`. El tiempo medio se marca como no disponible. Se revisará en el 3a. |
| A5 | **Programar publicaciones** | Instagram y TikTok no tienen programación nativa por API, y en YouTube depende de la auditoría (R4). | Habrá un **programador local**: Omnyra publica a la hora indicada, y para eso tiene que estar abierta o en la bandeja. Si el PC está apagado, se avisa al volver y se ofrece "publicar ahora". Hace falta **arranque automático al iniciar sesión** (hueco H3). |
| A6 | Instagram: **subida de vídeo** | Históricamente, la Graph API exigía que el vídeo estuviera en una **URL pública**. Ahora existe una subida reanudable para Reels (`rupload`). | Usar la subida reanudable. Si no fuera posible, haría falta almacenamiento temporal público, lo que influye en la decisión del 3a. |
| A7 | X | La API gratuita apenas permite leer métricas. El nivel de pago es caro y los precios cambian a menudo. | Ver H1. |
| A8 | **Competencia y tendencias** | YouTube: sí (estadísticas públicas, pero con coste de cuota). Instagram: *Business Discovery* solo para cuentas profesionales. TikTok: nada (la Research API es solo para el ámbito académico). Google Trends no tiene API pública estable. | Fase 6b con YouTube primero e Instagram como extra. Las "tendencias" se basan en búsquedas de YouTube y en `videos.list?chart=mostPopular` por categoría. |
| A9 | **Detección de suplantación por avatar** | Las búsquedas de cuentas por API son muy limitadas (YouTube `search.list type=channel` sí, a 100 unidades por llamada). | Se compara el nombre (distancia de edición, homóglifos) y el avatar (hash perceptual, `pHash`) **solo sobre resultados de búsqueda y autores de comentarios**. Nada de rastrear webs (scraping). |

### 1.3 Huecos del spec

- **H1 — X no tiene paso en la hoja de ruta.** Aparece en las plataformas objetivo, pero ninguna fase lo implementa. Propuesta: un paso opcional **3d — Conector de X** (Opus), condicionado a que el usuario quiera pagar la API. **[DECIDIR]**
- **H2 — Varias cuentas por red.** El spec habla de "una entrada por red conectada". El modelo de datos admitirá **N cuentas por plataforma** desde el principio, aunque no cuesta casi nada. En la barra lateral, las cuentas se agrupan bajo su red.
- **H3 — Arranque automático.** Los snapshots y el programador necesitan que la app arranque con la sesión, con `tauri-plugin-autostart` y la opción desactivada por defecto. No aparece en el spec; se añade al paso 2c.
- **H4 — Bloqueo con PIN sin cifrado = cosmético.** Si SQLite no está cifrada, el PIN solo protege la UI: cualquiera con acceso al disco lee los comentarios, los patrocinios y los importes. Opciones: (a) dejarlo claro en la UI, o (b) cifrar la base con **SQLCipher** usando una clave en el llavero o derivada del PIN. **[DECIDIR]** en el 6c. La arquitectura deja preparada la opción (b): la conexión se abre en un único sitio.
- **H5 — Llavero no disponible en Linux.** En escritorios mínimos (Hyprland, i3…) a veces no hay Secret Service. *En tu sistema sí funciona: `gnome-keyring-daemon` sirve `org.freedesktop.secrets`.* Comportamiento propuesto: si no hay llavero, **no se guardan los tokens** y se muestra cómo instalar o activar uno (sin volver nunca a texto plano). Una bóveda cifrada con contraseña maestra queda como posible añadido. **[DECIDIR]**
- **H6 — La URL de sesión de subida reanudable es un credencial.** La URI de sesión de YouTube permite terminar la subida sin token durante días. Se guarda en el **llavero**, no en SQLite.
- **H7 — Backup y llavero.** El backup de SQLite no incluye los tokens, así que al restaurar en otro equipo hay que reconectar las cuentas. Se documenta en la UI.
- **H8 — Zona horaria.** Todo se guarda en **UTC**. "Mejor hora" y el calendario se calculan en la zona del usuario (configurable). La cuota de YouTube se reinicia en hora del Pacífico.
- **H9 — Licencia de FFmpeg.** Para incluirlo en Windows hace falta una compilación **LGPL** (sin `libx264`/GPL), o descargarlo en el primer arranque. En Linux es una dependencia del paquete (`ffmpeg`). En AppImage se usa el del sistema y, si no está, se muestra un aviso. **[DECIDIR]** en el 0c. Recomendación: en Windows, descarga verificada por SHA-256 en el primer uso, y ruta configurable.
- **H10 — Pruebas en Windows.** El spec pide probar el llavero y las rutas en Windows. Si no tienes un PC con Windows, usaremos la VM de VirtualBox que ya tienes junto con la CI.
- **H11 — Bandeja del sistema en Wayland/Hyprland.** Necesita un servidor StatusNotifierItem. *En tu sistema existe (`org.kde.StatusNotifierWatcher` de quickshell).* En otros escritorios sin bandeja, la opción "seguir en segundo plano" se oculta.
- **H12 — Clave de API del LLM (5b).** Va al llavero, igual que los tokens. Antes de enviar datos al LLM, el usuario elige qué se manda.
- **H13 — Node 26.** Tauri 2 funciona con cualquier Node ≥ 18. Si la CI usa la LTS anterior no pasa nada; se fija la versión con `.nvmrc`.
- **H14 — Rust no está instalado** (pendiente del paso 0.0): `sudo pacman -S rustup && rustup default stable`.

### 1.4 Pequeñas inconsistencias

- La sección 3 permite `tauri-plugin-sql` **o** `rusqlite`. Se elige **`rusqlite`** (ver 3.1): el plugin SQL expone SQL arbitrario al frontend, lo que amplía la superficie de ataque, y los trabajos en segundo plano necesitan la base de datos en Rust de todos modos.
- El paso 1e pide la "vista global" antes de tener una segunda red. Funciona igual con una sola red; es solo para que quede claro.
- "Tests en Windows y en Linux" en el 1a: los tests automáticos del llavero usan un *mock*. La prueba real en cada sistema es manual y se apunta como checklist en `PROGRESS.md`.

---

## 2. Visión general

```
┌──────────────────────────── Frontend (WebView) ────────────────────────────┐
│ React + TS + Vite · Tailwind + shadcn/ui · TanStack Query · Zustand · i18n │
│ Solo presentación + editor de miniaturas (canvas). NUNCA ve tokens.        │
└───────────────▲──────────────────────────────┬─────────────────────────────┘
                │ eventos / Channels           │ commands tipados (tauri-specta)
┌───────────────┴──────────────────────────────▼─────────────────────────────┐
│ src-tauri (capa fina): registro de commands, tray, plugins, permisos       │
├────────────────────────────────────────────────────────────────────────────┤
│ crates/omnyra-core (Rust puro, testeable sin Tauri ni WebKit)              │
│  auth (OAuth+PKCE, loopback, TokenStore→keyring)                           │
│  connectors (trait Connector + registry + http con backoff y cuota)        │
│  jobs (cola persistente: sync, snapshot, upload, publish, render)          │
│  db (rusqlite, migraciones, repositorios)                                  │
│  media (ffprobe/ffmpeg, presets por red)                                   │
│  analytics (agregación, mejor hora, alertas, momentos)                     │
│  community (clasificación, agrupación de preguntas)                        │
│  security (phishing, suplantación, lock, audit log)                        │
└────────────────────────────────────────────────────────────────────────────┘
        │ HTTPS (reqwest + rustls)          │ keyring          │ SQLite + archivos
     APIs de redes                     Credential Manager /   app_data_dir/
                                       Secret Service         omnyra.db, files/, cache/
```

### 2.1 Qué va en Rust y qué en el frontend

**Regla general:** todo lo que toque secretos, red, sistema de archivos, base de datos o trabajos largos va en **Rust**. El frontend presenta los datos y edita en local.

| Rust (`omnyra-core`) | Frontend |
|---|---|
| OAuth, tokens y llavero | Pantallas, gráficas y formularios |
| Todas las llamadas a APIs de redes | Validación de formularios en vivo (la validación definitiva se repite en Rust) |
| SQLite (lectura y escritura) | Estado de la UI (Zustand): tema, filtros, selección |
| Snapshots y sincronización en segundo plano | Editor de miniaturas (canvas). Exporta un PNG que Rust guarda. |
| Cola de subidas y programador de publicaciones | Vista previa de cómo quedará en cada red |
| FFmpeg / ffprobe | Arrastrar y soltar (calendario, Kanban) |
| Algoritmos: mejor hora, alertas, clasificación, phishing, momentos | Plantillas de texto (5b): ambas partes. Las plantillas se renderizan en TS; la llamada al LLM, en Rust. |
| Generación del media kit (HTML) | |

**Por qué los algoritmos van en Rust:** se ejecutan en segundo plano sin UI (las alertas y las notificaciones se calculan tras un snapshot aunque la ventana esté cerrada), se prueban con `cargo test` y no se duplican.

### 2.2 Comunicación Tauri

- **Commands** (petición y respuesta): todos los `#[tauri::command]` viven en `src-tauri/src/commands/*` y solo llaman a `omnyra-core`. Los tipos de Rust se exportan a TypeScript con **`tauri-specta`**, que genera `src/lib/bindings.ts`. Así no hay tipos escritos a mano duplicados. *(Dependencia nueva, pero ligera. **[DECIDIR]**: es la opción recomendada.)*
- **Channels** (`tauri::ipc::Channel`) para progreso continuo con un solo destinatario: el porcentaje de subida, el progreso de FFmpeg y el progreso de la sincronización.
- **Eventos globales** (`app.emit`) para cambios que interesan a varias pantallas:
  - `sync://status` `{ accountId, state, lastSyncAt }`
  - `jobs://updated` `{ jobId, kind, status }`
  - `alerts://new` `{ alertId }`
  - `auth://account-changed` `{ accountId, status }` (por ejemplo, un token caducado)
  - El frontend responde a esos eventos invalidando las *queries* de TanStack Query afectadas.
- **Errores:** un único `enum AppError` serializable con un `code` estable (`TOKEN_EXPIRED`, `QUOTA_EXCEEDED`, `RATE_LIMITED{retryAfter}`, `NETWORK`, `NOT_SUPPORTED`, `VALIDATION{field}`…). El frontend traduce `code` a un mensaje i18n. Los mensajes técnicos nunca incluyen tokens.
- **Seguridad del IPC:** capacidades de Tauri 2 (`src-tauri/capabilities/`) con solo los plugins necesarios. `fs` limitado a `$APPDATA/omnyra` y a los archivos que el usuario elija mediante el diálogo. CSP estricta: `img-src 'self' asset: data:` sin CDNs externos (las miniaturas remotas se descargan a la caché local con Rust, que además sirve para trabajar sin conexión).

### 2.3 Plugins de Tauri previstos

`opener` (abrir el navegador para OAuth), `dialog`, `fs` (con ámbito limitado), `notification`, `single-instance` (imprescindible con la bandeja y el deep link), `window-state`, `autostart` (paso 2c), `clipboard-manager` (para "Preparar y descargar"), `log`, `updater` (paso 7c) y `deep-link` (solo si lo decide el 3a).

---

## 3. Sistema de conectores

### 3.1 Trait en Rust

```rust
pub struct PlatformInfo {
    pub id: PlatformId,           // "youtube", "twitch", "mock"...
    pub display_name: &'static str,
    pub brand_color: &'static str,
    pub capabilities: Capabilities,
    pub oauth: Option<OAuthConfig>,  // endpoints, scopes por funcionalidad
}

pub struct Capabilities {
    pub analytics: bool,
    pub historic_analytics: bool,  // permite rellenar huecos (YouTube Analytics)
    pub upload: UploadSupport,     // None | Direct | DraftOnly | PrivateOnlyUnaudited
    pub resumable_upload: bool,
    pub schedule: ScheduleSupport, // None | Native | LocalOnly
    pub comments: CommentSupport,  // None | Read | ReadReply
    pub live: bool,
    pub public_lookup: bool,       // competencia / suplantación
    pub limits: MediaLimits,       // duración, peso, aspectos, formatos, longitud de texto, nº de hashtags
}

#[async_trait]
pub trait Connector: Send + Sync {
    fn info(&self) -> &PlatformInfo;

    async fn fetch_profile(&self, ctx: &AccountCtx) -> Result<Profile>;
    async fn fetch_channel_metrics(&self, ctx: &AccountCtx, range: DateRange) -> Result<Vec<MetricPoint>>;
    async fn list_contents(&self, ctx: &AccountCtx, page: PageToken) -> Result<Page<Content>>;
    async fn fetch_content_metrics(&self, ctx: &AccountCtx, ids: &[ExternalId]) -> Result<Vec<ContentMetrics>>;

    // Opcionales: la implementación por defecto devuelve AppError::NotSupported
    async fn upload(&self, ctx: &AccountCtx, req: UploadRequest, progress: ProgressSink) -> Result<UploadOutcome>;
    async fn list_comments(&self, ctx: &AccountCtx, content: &ExternalId, page: PageToken) -> Result<Page<Comment>>;
    async fn reply_comment(&self, ctx: &AccountCtx, parent: &ExternalId, text: &str) -> Result<Comment>;
    async fn content_series(&self, ctx: &AccountCtx, content: &ExternalId, kind: SeriesKind) -> Result<Series>; // retención...
    async fn lookup_public(&self, ctx: &AccountCtx, query: PublicQuery) -> Result<Vec<PublicAccount>>;
}
```

- **La autenticación no forma parte del trait.** La hace un módulo genérico `auth` a partir de `OAuthConfig`: PKCE, loopback, refresco y revocación. Así el 2a reutiliza el 1a sin tocarlo, como pide el spec. Las plataformas con un flujo especial (Meta y TikTok, 3a) implementan un `AuthFlow` alternativo.
- `AccountCtx` da al conector un **cliente HTTP ya autenticado** (que inyecta el token y refresca si recibe un 401), el limitador de peticiones y el registro de cuota. **El conector nunca toca el llavero.**
- **Métricas como clave y valor** (`MetricPoint { key: MetricKey, value: f64, day }`). `MetricKey` combina claves comunes (`followers`, `views`, `likes`, `comments`, `shares`, `watch_time_s`, `impressions`…) con claves de plataforma (`yt.avg_view_pct`, `twitch.peak_viewers`…). El esquema de la base de datos no cambia al añadir una red.
- **Registro:** `connectors/registry.rs` tiene la lista de conectores. Añadir una red supone **una carpeta nueva y una línea en el registro**, en Rust y en el frontend. Es lo mínimo razonable sin usar plugins dinámicos (que no compensan aquí).
- **Ajustes de cada red:** `platform_options: serde_json::Value` validado por el propio conector (categoría de YouTube, *made for kids*, privacidad de TikTok, etc.). En el frontend, cada red aporta su componente `UploadOptionsForm`.

### 3.2 Lado del frontend

```ts
// src/platforms/registry.ts
export interface PlatformUI {
  id: PlatformId;
  icon: React.FC; color: string;
  AnalyticsView?: React.FC<{ accountId: string }>;     // analíticas específicas
  UploadOptionsForm?: React.FC<UploadOptionsProps>;    // opciones de subida específicas
  PreviewCard?: React.FC<PreviewProps>;                // vista previa de cómo quedará
}
```

Si una red no aporta alguno de estos componentes, se usan vistas genéricas a partir de las `Capabilities` que devuelve Rust con `list_platforms()`. Sin `upload`, se muestra "Preparar y descargar" en lugar de "Publicar".

### 3.3 HTTP, límites y errores

- `reqwest` + `rustls`. Un **cliente por plataforma** con:
  - limitador de peticiones (token bucket, crate `governor`);
  - reintentos con **backoff exponencial y jitter**, solo en peticiones idempotentes y en errores 429/5xx, respetando `Retry-After`;
  - registro de cuota (`quota_ledger`), con un coste por endpoint declarado en el conector;
  - caché con ETag (`api_cache`) para perfiles y listas que cambian poco.
- Los logs pasan por un filtro que **elimina** `Authorization`, `access_token`, `refresh_token`, `code`, `code_verifier` y las URIs de sesión de subida.

### 3.4 OAuth

1. Se genera `code_verifier`, `state` y `nonce` aleatorios (con `rand::rngs::OsRng`).
2. Se abre un servidor de loopback en `127.0.0.1:0` (puerto aleatorio) que acepta **una sola** petición a `/callback`, valida `state`, responde con una página simple de "Ya puedes cerrar esta pestaña" y se cierra. Tiene un tiempo límite de 5 minutos.
3. Se intercambia el `code` por los tokens, que se guardan en `TokenStore` (keyring) con la clave `omnyra/<account_uuid>`. El valor es un JSON `{access, refresh, expires_at, scopes}`.
4. **Scopes mínimos e incrementales:** al conectar se piden solo los de lectura. `youtube.upload` y `youtube.force-ssl` (comentarios) se piden **la primera vez que se usa esa funcionalidad**, con `include_granted_scopes`. La pantalla "Cuentas conectadas" muestra los scopes concedidos.
5. Al desconectar: se revoca el token en la plataforma, se borra la entrada del llavero y la cuenta se marca como `disconnected`. Los datos históricos se conservan, salvo que el usuario pida borrarlos.

`TokenStore` es un trait con dos implementaciones: `KeyringStore` (la real) y `MemoryStore` (para tests).

---

## 4. Segundo plano y trabajos

- **Un único planificador** (tokio) arranca con la app y procesa la tabla `jobs`. Es una cola persistente, así que sobrevive a reinicios.
- Tipos de trabajo: `sync_account`, `snapshot_daily`, `backfill_history`, `upload`, `publish_scheduled`, `render_rendition`, `fetch_comments`, `live_sample`, `competitor_snapshot`.
- **Concurrencia:** subidas, 1 por red y 2 en total; renderizados FFmpeg, 1 (son pesados); sincronizaciones, 3.
- Estados: `pending → running → done | failed(retry_at) | cancelled`. Hasta 5 reintentos con backoff, y después se crea una alerta para el usuario.
- **Al abrir la app:** se recuperan los trabajos que se quedaron `running` (por un cierre brusco), se programa un `sync_account` para cada cuenta y un `backfill_history` si faltan días.
- **Cerrar la ventana:** si "seguir en segundo plano" está activado, la ventana se oculta y el planificador sigue funcionando (bandeja). Si no, la app se cierra.

---

## 5. Esquema de SQLite

Convenciones: IDs `TEXT` (UUID v7, ordenables por fecha). Fechas con hora en `TEXT` ISO-8601 **UTC**. Días en `TEXT 'YYYY-MM-DD'` (UTC). Dinero en `INTEGER` (céntimos) + `currency`. JSON en `TEXT`. `PRAGMA foreign_keys=ON`, `journal_mode=WAL`. Migraciones en `crates/omnyra-core/src/db/migrations/NNNN_nombre.sql`, aplicadas con `rusqlite_migration` usando `user_version`.

La migración **0001** crea las tablas de la Fase 0–1 (marcadas con ★). Las demás llegan en migraciones posteriores, en el paso que las use, para no congelar un diseño que aún no se ha probado. Aquí aparecen todas para validar el modelo completo.

### 5.1 Cuentas y contenidos

```sql
-- ★ Cuentas conectadas (N por plataforma). SIN tokens.
CREATE TABLE accounts (
  id            TEXT PRIMARY KEY,
  platform      TEXT NOT NULL,              -- 'youtube' | 'twitch' | ... | 'mock'
  external_id   TEXT NOT NULL,
  handle        TEXT,
  display_name  TEXT NOT NULL,
  avatar_file_id TEXT REFERENCES files(id),
  granted_scopes TEXT NOT NULL DEFAULT '[]', -- JSON, solo para mostrarlo
  status        TEXT NOT NULL DEFAULT 'active', -- active | needs_reauth | disconnected
  connected_at  TEXT NOT NULL,
  last_sync_at  TEXT,
  settings      TEXT NOT NULL DEFAULT '{}',
  UNIQUE (platform, external_id)
);

-- ★ Contenidos publicados (traídos de la API)
CREATE TABLE contents (
  id            TEXT PRIMARY KEY,
  account_id    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  external_id   TEXT NOT NULL,
  kind          TEXT NOT NULL,   -- video | short | reel | post | story | stream | vod | clip | tweet
  title         TEXT,
  description   TEXT,
  url           TEXT,
  thumbnail_file_id TEXT REFERENCES files(id),
  published_at  TEXT,
  duration_s    INTEGER,
  aspect        TEXT,            -- '16:9' | '9:16' | '1:1' | ...
  visibility    TEXT,
  extra         TEXT NOT NULL DEFAULT '{}',  -- campos propios de la red
  first_seen_at TEXT NOT NULL,
  last_fetched_at TEXT NOT NULL,
  UNIQUE (account_id, external_id)
);
CREATE INDEX idx_contents_account_pub ON contents(account_id, published_at);

-- ★ Snapshots de cuenta (formato largo: añadir métricas no cambia el esquema)
CREATE TABLE account_snapshots (
  account_id  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  day         TEXT NOT NULL,
  metric      TEXT NOT NULL,   -- 'followers', 'views', 'yt.watch_time_s'...
  value       REAL NOT NULL,
  source      TEXT NOT NULL,   -- 'current' | 'historic' | 'manual'
  captured_at TEXT NOT NULL,
  PRIMARY KEY (account_id, metric, day)        -- upsert: el último del día gana
) WITHOUT ROWID;

-- ★ Snapshots por contenido
CREATE TABLE content_snapshots (
  content_id  TEXT NOT NULL REFERENCES contents(id) ON DELETE CASCADE,
  day         TEXT NOT NULL,
  metric      TEXT NOT NULL,
  value       REAL NOT NULL,
  source      TEXT NOT NULL,
  captured_at TEXT NOT NULL,
  PRIMARY KEY (content_id, metric, day)
) WITHOUT ROWID;

-- Series dentro de un contenido (curva de retención, fuentes de tráfico...)
CREATE TABLE content_series (
  content_id TEXT NOT NULL REFERENCES contents(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL,    -- 'retention' | 'traffic_sources' | ...
  data       TEXT NOT NULL,    -- JSON [{x, y}] o {clave: valor}
  fetched_at TEXT NOT NULL,
  PRIMARY KEY (content_id, kind)
) WITHOUT ROWID;

-- Comparativa entre redes: "el mismo contenido"
CREATE TABLE content_groups (
  id TEXT PRIMARY KEY, title TEXT NOT NULL, created_at TEXT NOT NULL
);
CREATE TABLE content_group_members (
  group_id   TEXT NOT NULL REFERENCES content_groups(id) ON DELETE CASCADE,
  content_id TEXT NOT NULL REFERENCES contents(id) ON DELETE CASCADE,
  PRIMARY KEY (group_id, content_id)
);

-- Directos (Twitch): muestreo propio, ver A2
CREATE TABLE live_sessions (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  external_id TEXT, title TEXT, category TEXT,
  started_at TEXT NOT NULL, ended_at TEXT,
  peak_viewers INTEGER, avg_viewers REAL, followers_gained INTEGER,
  vod_content_id TEXT REFERENCES contents(id)
);
CREATE TABLE live_samples (
  session_id TEXT NOT NULL REFERENCES live_sessions(id) ON DELETE CASCADE,
  ts TEXT NOT NULL, viewers INTEGER, chat_msgs INTEGER, -- chat_msgs por cubo de 10 s
  PRIMARY KEY (session_id, ts)
) WITHOUT ROWID;
```

### 5.2 Publicación

```sql
-- ★ Archivos gestionados por Omnyra (dentro de app_data_dir/files o referencias externas)
CREATE TABLE files (
  id         TEXT PRIMARY KEY,
  path       TEXT NOT NULL,     -- relativa a app_data_dir si managed=1; absoluta si no
  managed    INTEGER NOT NULL,  -- 1 = copiado a Omnyra, 0 = referencia al original (vídeos grandes)
  original_name TEXT, mime TEXT, size_bytes INTEGER, sha256 TEXT,
  media_info TEXT,              -- JSON de ffprobe (duración, resolución, códecs)
  created_at TEXT NOT NULL
);

-- ★ Lo que el usuario quiere publicar. Una fila por (vídeo × cuenta de destino).
CREATE TABLE publications (
  id            TEXT PRIMARY KEY,
  batch_id      TEXT,           -- agrupa la publicación multi-red (3c)
  account_id    TEXT NOT NULL REFERENCES accounts(id),
  source_file_id TEXT REFERENCES files(id),
  rendition_file_id TEXT REFERENCES files(id),  -- versión adaptada por FFmpeg
  thumbnail_file_id TEXT REFERENCES files(id),
  idea_id       TEXT REFERENCES ideas(id) ON DELETE SET NULL,
  title TEXT, description TEXT,
  tags          TEXT NOT NULL DEFAULT '[]',
  visibility    TEXT,
  platform_options TEXT NOT NULL DEFAULT '{}',
  status        TEXT NOT NULL,  -- draft | ready | queued | uploading | processing | scheduled | published | exported | failed
  schedule_mode TEXT,           -- native | local | NULL
  scheduled_at  TEXT,
  published_content_id TEXT REFERENCES contents(id),
  error_code TEXT, error_detail TEXT,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX idx_publications_sched ON publications(status, scheduled_at);

-- ★ Cola de trabajos persistente (sección 4)
CREATE TABLE jobs (
  id          TEXT PRIMARY KEY,
  kind        TEXT NOT NULL,
  ref_id      TEXT,             -- account_id / publication_id / ...
  payload     TEXT NOT NULL DEFAULT '{}',  -- nunca secretos (la URI de sesión de subida va al llavero, H6)
  status      TEXT NOT NULL,    -- pending | running | done | failed | cancelled
  attempts    INTEGER NOT NULL DEFAULT 0,
  run_after   TEXT NOT NULL,
  progress    REAL,
  last_error  TEXT,
  created_at  TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX idx_jobs_pending ON jobs(status, run_after);

-- ★ Registro de cuota (R5)
CREATE TABLE quota_ledger (
  platform TEXT NOT NULL, quota_day TEXT NOT NULL, -- día en la zona horaria de la plataforma
  units_used INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (platform, quota_day)
) WITHOUT ROWID;

-- ★ Caché HTTP (ETag)
CREATE TABLE api_cache (
  key TEXT PRIMARY KEY, etag TEXT, body BLOB, fetched_at TEXT NOT NULL, expires_at TEXT
);
```

### 5.3 Planificación

```sql
CREATE TABLE ideas (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  notes TEXT,                     -- Markdown
  stage TEXT NOT NULL DEFAULT 'idea', -- idea | script | recorded | edited | published
  position REAL NOT NULL,         -- orden dentro de la columna (fraccional: reordenar sin reescribir)
  target_platforms TEXT NOT NULL DEFAULT '[]',
  due_at TEXT,
  source_cluster_id TEXT REFERENCES question_clusters(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE idea_files (
  idea_id TEXT NOT NULL REFERENCES ideas(id) ON DELETE CASCADE,
  file_id TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  PRIMARY KEY (idea_id, file_id)
);
-- Recordatorios locales. El calendario es la UNIÓN de:
-- contents.published_at + publications.scheduled_at + reminders.at + ideas.due_at
CREATE TABLE reminders (
  id TEXT PRIMARY KEY, title TEXT NOT NULL, at TEXT NOT NULL,
  platform TEXT, idea_id TEXT REFERENCES ideas(id) ON DELETE CASCADE,
  publication_id TEXT REFERENCES publications(id) ON DELETE CASCADE,
  notified_at TEXT
);
```

### 5.4 Comunidad

```sql
CREATE TABLE comments (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  content_id TEXT REFERENCES contents(id) ON DELETE CASCADE,
  external_id TEXT NOT NULL,
  parent_id TEXT REFERENCES comments(id) ON DELETE CASCADE,
  author_name TEXT, author_external_id TEXT, author_avatar_url TEXT,
  is_own INTEGER NOT NULL DEFAULT 0,      -- respuesta escrita por el creador
  text TEXT NOT NULL,
  published_at TEXT, like_count INTEGER,
  status TEXT NOT NULL DEFAULT 'pending', -- pending | replied | archived
  category TEXT,                          -- question | spam | negative | positive | neutral
  category_source TEXT,                   -- rules | model | manual
  phishing_score REAL, phishing_reasons TEXT, -- JSON
  cluster_id TEXT REFERENCES question_clusters(id) ON DELETE SET NULL,
  fetched_at TEXT NOT NULL,
  UNIQUE (account_id, external_id)
);
CREATE INDEX idx_comments_inbox ON comments(status, published_at);
CREATE TABLE question_clusters (
  id TEXT PRIMARY KEY, label TEXT NOT NULL, size INTEGER NOT NULL,
  updated_at TEXT NOT NULL
);
-- Búsqueda de texto en comentarios
CREATE VIRTUAL TABLE comments_fts USING fts5(text, content='comments', content_rowid='rowid');
```

### 5.5 Creación, monetización, competencia y seguridad

```sql
-- Biblioteca de recursos (skins de gh0stymc, avatares, stickers...)
CREATE TABLE assets (
  id TEXT PRIMARY KEY, file_id TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,          -- avatar | skin | sticker | background | font | image
  name TEXT NOT NULL, variant TEXT, tags TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL
);
CREATE TABLE thumbnail_templates (
  id TEXT PRIMARY KEY, name TEXT NOT NULL,
  width INTEGER NOT NULL, height INTEGER NOT NULL,
  scene TEXT NOT NULL,         -- JSON del editor (capas)
  preview_file_id TEXT REFERENCES files(id),
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE text_templates (
  id TEXT PRIMARY KEY, platform TEXT, field TEXT NOT NULL, -- title | description | hashtags
  name TEXT NOT NULL, body TEXT NOT NULL
);

CREATE TABLE sponsorships (
  id TEXT PRIMARY KEY, brand TEXT NOT NULL,
  contact_name TEXT, contact_email TEXT,
  status TEXT NOT NULL,          -- prospect | negotiating | signed | in_progress | delivered | cancelled
  amount_cents INTEGER, currency TEXT,
  payment_status TEXT NOT NULL DEFAULT 'pending', -- pending | invoiced | paid | overdue
  start_date TEXT, end_date TEXT, notes TEXT,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE sponsorship_deliverables (
  id TEXT PRIMARY KEY,
  sponsorship_id TEXT NOT NULL REFERENCES sponsorships(id) ON DELETE CASCADE,
  description TEXT NOT NULL, platform TEXT, due_at TEXT,
  done INTEGER NOT NULL DEFAULT 0,
  publication_id TEXT REFERENCES publications(id) ON DELETE SET NULL
);
CREATE TABLE sponsorship_files (
  sponsorship_id TEXT NOT NULL REFERENCES sponsorships(id) ON DELETE CASCADE,
  file_id TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  PRIMARY KEY (sponsorship_id, file_id)
);

CREATE TABLE tracked_accounts (           -- competencia
  id TEXT PRIMARY KEY, platform TEXT NOT NULL, external_id TEXT NOT NULL,
  handle TEXT, display_name TEXT, added_at TEXT NOT NULL,
  UNIQUE (platform, external_id)
);
CREATE TABLE tracked_snapshots (
  tracked_id TEXT NOT NULL REFERENCES tracked_accounts(id) ON DELETE CASCADE,
  day TEXT NOT NULL, metric TEXT NOT NULL, value REAL NOT NULL,
  PRIMARY KEY (tracked_id, metric, day)
) WITHOUT ROWID;
CREATE TABLE tracked_contents (
  id TEXT PRIMARY KEY,
  tracked_id TEXT NOT NULL REFERENCES tracked_accounts(id) ON DELETE CASCADE,
  external_id TEXT NOT NULL, title TEXT, published_at TEXT,
  views INTEGER, likes INTEGER, comments INTEGER, fetched_at TEXT NOT NULL,
  UNIQUE (tracked_id, external_id)
);
CREATE TABLE trend_keywords (
  id TEXT PRIMARY KEY, keyword TEXT NOT NULL, platform TEXT, created_at TEXT NOT NULL
);
CREATE TABLE trend_results (
  keyword_id TEXT NOT NULL REFERENCES trend_keywords(id) ON DELETE CASCADE,
  day TEXT NOT NULL, data TEXT NOT NULL,
  PRIMARY KEY (keyword_id, day)
) WITHOUT ROWID;

-- ★ Alertas (rendimiento, hitos, token caducado, fallo de subida, phishing...)
CREATE TABLE alerts (
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, severity TEXT NOT NULL,
  account_id TEXT REFERENCES accounts(id) ON DELETE CASCADE,
  content_id TEXT REFERENCES contents(id) ON DELETE CASCADE,
  payload TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL, read_at TEXT, notified_at TEXT
);

CREATE TABLE security_checklist (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  item TEXT NOT NULL,            -- '2fa' | 'recovery_email' | 'third_party_apps' ...
  state TEXT NOT NULL,           -- unknown | ok | warning | not_applicable
  source TEXT NOT NULL,          -- api | manual
  checked_at TEXT,
  PRIMARY KEY (account_id, item)
) WITHOUT ROWID;
CREATE TABLE impersonation_hits (
  id TEXT PRIMARY KEY, account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  platform TEXT NOT NULL, suspect_external_id TEXT NOT NULL, suspect_name TEXT,
  name_score REAL, avatar_score REAL,
  status TEXT NOT NULL DEFAULT 'new', -- new | dismissed | reported
  found_at TEXT NOT NULL,
  UNIQUE (account_id, platform, suspect_external_id)
);

-- ★ Registro de acciones sensibles (solo se añaden filas; los triggers impiden UPDATE y DELETE)
CREATE TABLE audit_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts TEXT NOT NULL,
  action TEXT NOT NULL,          -- account.connect | account.disconnect | scope.grant | publish | reply | lock.change | backup.export
  account_id TEXT,
  detail TEXT NOT NULL DEFAULT '{}', -- sin secretos
  prev_hash TEXT, hash TEXT      -- cadena de hashes: muestra si se ha manipulado
);

-- ★ Ajustes
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
```

### 5.6 Archivos en disco

```
<app_data_dir>/            Linux: ~/.local/share/com.omnyra.app   Windows: %APPDATA%\com.omnyra.app
  omnyra.db
  files/<aa>/<uuid>.<ext>  archivos gestionados (recursos, contratos, miniaturas, renders)
  cache/thumbs/            miniaturas remotas cacheadas (permiten trabajar sin conexión)
  logs/                    con rotación y filtro de secretos
```

Las rutas se obtienen siempre con `app.path()` de Tauri. Nunca se escriben a mano, así funcionan igual en Windows y Linux.

---

## 6. Algoritmos (resumen; el detalle va en su paso)

- **Mejor hora (1c):** para cada contenido, `score = log(views_at_7d) − media(log(views_at_7d))` del canal, separando Shorts y vídeos largos. Se usan las visitas a 7 días según los snapshots, o la Analytics API cuando haya histórico, y se pondera por antigüedad (`w = 0.5^(age_days/180)`). Se agrega por (día, hora local) con suavizado bayesiano hacia la media (pocos datos → valor cercano a 0). Si hay menos de 15 contenidos o una celda tiene un peso total bajo, se muestra "datos insuficientes".
- **Alertas:** puntuación z sobre las visitas a 48 h frente a los últimos 20 contenidos del mismo tipo; hitos de seguidores (múltiplos de 10^k y de 5·10^k); caída de engagement mediante una media móvil de 4 semanas.
- **Clasificación de comentarios (4b):** reglas por idioma (ES/EN) con interrogaciones, palabras interrogativas, listas de spam y patrones de enlaces. Hay un trait `Classifier` para conectar un modelo más adelante.
- **Preguntas repetidas:** normalización, *shingles* de 3 caracteres, MinHash/LSH y agrupación con umbral de Jaccard.
- **Phishing (6c):** extracción de URLs, incluidas las ofuscadas ("t . me", "hxxp"); dominios parecidos a una lista de marcas mediante distancia de Damerau-Levenshtein y esqueleto de confusables Unicode (UTS #39); TLDs de riesgo; acortadores; y patrones de estafa ("sorteo", "escríbeme por Telegram/WhatsApp", "has ganado", criptomonedas). La puntuación es explicable (`reasons[]`).
- **Momentos (5c):** detección de picos en la curva de retención (pendiente positiva o relativa a la media) y en la serie de chat (cubos de 10 s, puntuación z robusta con mediana y MAD); fusión de ventanas cercanas; propuestas de 15–60 s.

---

## 7. Estructura de carpetas

```
omnyra/
├─ OMNYRA_SPEC.md
├─ PROGRESS.md                      (paso 0b)
├─ README.md · CHANGELOG.md · LICENSE
├─ .env.example                     (client IDs; .env en .gitignore)
├─ .nvmrc · rust-toolchain.toml
├─ package.json · vite.config.ts · tsconfig.json · eslint.config.js · .prettierrc
├─ tailwind.config.ts · components.json (shadcn)
├─ Cargo.toml                       (workspace: crates/omnyra-core, src-tauri)
├─ crates/
│  └─ omnyra-core/                  Rust puro, sin Tauri → cargo test rápido en la CI
│     └─ src/
│        ├─ lib.rs · error.rs · clock.rs (reloj inyectable para tests)
│        ├─ db/        mod.rs · migrations/0001_init.sql · repo/{accounts,contents,snapshots,...}.rs
│        ├─ auth/      oauth.rs · pkce.rs · loopback.rs · token_store.rs
│        ├─ connectors/ mod.rs (trait, Capabilities) · registry.rs · http.rs (backoff, cuota, ETag)
│        │             mock/ · youtube/ · twitch/ · instagram/ · tiktok/ · x/
│        ├─ jobs/      scheduler.rs · sync.rs · snapshot.rs · upload.rs · publish.rs
│        ├─ media/     ffmpeg.rs · probe.rs · presets.rs
│        ├─ analytics/ aggregate.rs · best_time.rs · alerts.rs · moments.rs
│        ├─ community/ classify.rs · cluster.rs
│        └─ security/  phishing.rs · impersonation.rs · lock.rs · audit_log.rs
├─ src-tauri/                       capa fina de Tauri
│  ├─ tauri.conf.json · Cargo.toml · build.rs
│  ├─ capabilities/default.json
│  ├─ icons/
│  └─ src/ main.rs · lib.rs (setup, plugins, estado) · tray.rs · events.rs
│          commands/{accounts,analytics,contents,publish,jobs,ideas,comments,creation,sponsors,security,settings}.rs
├─ src/                             frontend
│  ├─ main.tsx · app/{router.tsx,providers.tsx,layout/Sidebar.tsx,...}
│  ├─ components/ui/                shadcn
│  ├─ components/                   comunes (charts, empty-states, date-range, sync-indicator)
│  ├─ lib/ bindings.ts (generado) · ipc.ts · queryKeys.ts · format.ts
│  ├─ stores/                       zustand (ui, filters)
│  ├─ i18n/ index.ts · es/*.json · en/*.json
│  ├─ platforms/ registry.ts · youtube/ · twitch/ · instagram/ · tiktok/ · x/ · mock/
│  └─ features/
│     ├─ dashboard/ · platform-view/ · calendar/ · ideas/ · comments/
│     ├─ creation/{thumbnails,text-assistant,moments}/
│     ├─ sponsors/ · competitors/ · security/ · settings/
├─ packaging/
│  ├─ arch/PKGBUILD · omnyra.install
│  └─ linux/com.omnyra.app.desktop · metainfo.xml
├─ scripts/ci/                      lógica de la CI independiente del proveedor (R1)
├─ .github/workflows/  (o .forgejo/workflows/)
├─ .claude/agents/ docs-writer.md · i18n-translator.md · test-writer.md · ui-builder.md
└─ docs/ ARCHITECTURE.md · AUTH_STRATEGY.md (3a) · REVIEW.md (7a) · USER_GUIDE.md (7d)
```

**Por qué hay un workspace con `omnyra-core` aparte:** la lógica (conectores, algoritmos, base de datos) se compila y se prueba sin WebKit ni Tauri, así que los tests son más rápidos y la CI más sencilla. `src-tauri` solo adapta: commands, bandeja y plugins.

---

## 8. Dependencias principales previstas

| Capa | Dependencia | Motivo |
|---|---|---|
| Rust | `tokio`, `reqwest` (rustls), `serde`, `thiserror`, `tracing` | Base |
| Rust | `rusqlite` (bundled) + `rusqlite_migration` | Base de datos (3.1) |
| Rust | `keyring` (v3, `windows-native` + `sync-secret-service`) | Llavero |
| Rust | `oauth2`, `governor`, `uuid` (v7), `time`, `sha2`, `url` | OAuth, límites, IDs |
| Rust | `tauri-specta` + `specta` | Tipos compartidos **[DECIDIR]** |
| Front | `react-router` v7, `@tanstack/react-query`, `zustand`, `i18next` + `react-i18next` | Spec |
| Front | `recharts` | Gráficas (el mapa de calor se hace a mano con SVG/CSS grid) |
| Front | `@dnd-kit/core` | Kanban y calendario. El calendario se hace a mano: hay vistas de mes y semana, y FullCalendar pesa mucho para esto |
| Front (fase 5) | `konva` + `react-konva` | Editor de miniaturas. Se consultará en el 5a |

Las dependencias pesadas o dudosas (generación de PDF del media kit, SQLCipher, detección de idioma) se consultan en su paso, como pide el spec.

---

## 9. Decisiones pendientes (resumen)

| # | Pregunta | Recomendación |
|---|---|---|
| D1 | ¿Aceptas el workspace `omnyra-core` + `src-tauri`? | Sí |
| D2 | ¿`tauri-specta` para generar los tipos TS? | Sí |
| D3 | `rusqlite` en lugar de `tauri-plugin-sql` | Sí |
| D4 | Sin llavero en Linux → error claro (sin bóveda propia por ahora) | Sí; la bóveda, más adelante si hace falta |
| D5 | Añadir el paso **3d — Conector de X** (opcional, si se paga la API) | Decidir cuando llegue la Fase 3 |
| D6 | Proveedor de CI si sigue la suspensión de GitHub | Codeberg/Forgejo o GitLab, decidir en el 0c |
| D7 | Bloqueo con PIN: ¿solo UI (avisándolo) o SQLCipher? | Decidir en el 6c; la arquitectura admite las dos |
| D8 | FFmpeg en Windows: ¿incluido (LGPL) o descargado en el primer arranque? | Descarga verificada en el primer arranque, decidir en el 0c |
| D9 | Pedir la auditoría de YouTube API tras el MVP | Sí, en cuanto el MVP funcione |
