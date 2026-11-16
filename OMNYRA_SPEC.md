# Omnyra — Especificación del proyecto

> Documento de referencia para Claude Code. Léelo entero antes de empezar y vuelve a él cuando tengas dudas sobre el alcance.

## 1. Qué es Omnyra

Omnyra es una **aplicación de escritorio para creadores de contenido** que reúne en un solo sitio la gestión de todas sus redes sociales (YouTube, TikTok, Instagram, X, Twitch y, más adelante, otras).

Tiene dos niveles:

1. **Vista global**: métricas unificadas, calendario común, publicación multi-red y herramientas transversales.
2. **Vista individual por red social**: analíticas específicas de esa plataforma, sus propias herramientas y **subida directa de vídeos** a esa red.

El autor es estudiante de ciberseguridad y creador de contenido de Minecraft (canal **gh0stymc**). La app debe ser útil para cualquier creador, pero la seguridad es un valor diferencial del producto.

## 2. Plataformas objetivo (requisito obligatorio)

La app debe funcionar y distribuirse en:

- **Windows 10/11** (x64) → instalador `.msi` y/o `.exe` (NSIS).
- **Linux Debian/Ubuntu y derivadas** → paquete `.deb`.
- **Linux Arch y derivadas** → `PKGBUILD` (preparado para AUR) y además un **AppImage** como opción universal.
- (Opcional) `.rpm` para Fedora, si el empaquetado lo da casi gratis.

Nada de la app puede depender de APIs exclusivas de un sistema operativo sin tener su alternativa en el otro. Prueba las rutas, permisos y el almacenamiento seguro en ambos sistemas.

## 3. Stack técnico recomendado

- **Framework de escritorio:** [Tauri 2](https://tauri.app) (backend en Rust y frontend web). Es ligero, multiplataforma y genera `.msi`, `.exe`, `.deb`, `.rpm` y AppImage de serie.
  - Si encuentras un bloqueo serio con Tauri, propón Electron como alternativa **antes** de cambiar, explicando el motivo.
- **Frontend:** React + TypeScript + Vite, Tailwind CSS y una librería de componentes (shadcn/ui o similar).
- **Gráficas:** Recharts o ECharts.
- **Estado/datos:** TanStack Query para las llamadas y Zustand para el estado de la UI.
- **Base de datos local:** SQLite (vía `tauri-plugin-sql` o `rusqlite`) con migraciones versionadas.
- **Procesado de vídeo:** FFmpeg (incluido en la app o detectado en el sistema; en Linux, declarado como dependencia del paquete).
- **Almacenamiento seguro de tokens:** el llavero del sistema operativo (crate `keyring`): Windows Credential Manager y, en Linux, Secret Service / libsecret (GNOME Keyring, KWallet).
- **Internacionalización:** i18n desde el principio, con **español** como idioma por defecto e **inglés** como segundo idioma.

## 4. Arquitectura

### 4.1 Sistema de conectores

Cada red social es un **conector** independiente que implementa una interfaz común (trait en Rust o interfaz en TS, según dónde viva la lógica):

```
Connector
  id, nombre, icono, color
  capacidades: { analytics, upload, schedule, comments, live }
  autenticar() / desconectar() / refrescarToken()
  obtenerPerfil()
  obtenerMetricasCanal(rango)
  obtenerContenidos(paginación)
  obtenerMetricasContenido(id)
  subirVideo(archivo, metadatos)       // si capacidades.upload
  obtenerComentarios(contenidoId)      // si capacidades.comments
  responderComentario(...)
```

- La UI se adapta a las **capacidades** de cada conector. Si una red no permite subir por API, la vista muestra "Preparar y descargar" en lugar de "Publicar".
- Añadir una red nueva no debe obligar a tocar el resto del código.

### 4.2 Snapshots de métricas

Muchas APIs solo devuelven el valor **actual** de seguidores y visitas. Para mostrar la evolución en el tiempo:

- Guarda **snapshots periódicos** en SQLite (como mínimo uno diario por cuenta y por contenido reciente).
- La app puede quedarse en la **bandeja del sistema** para tomar snapshots en segundo plano (configurable).
- Al abrir la app, rellena los huecos donde la API permita consultar datos históricos (por ejemplo, YouTube Analytics API).

### 4.3 Autenticación (OAuth)

- Usa **OAuth 2.0 con PKCE** y redirección a **loopback** (`http://127.0.0.1:<puerto aleatorio>`) cuando la plataforma lo admita (Google lo admite para apps de escritorio).
- Para plataformas que exijan `client_secret` o URIs de redirección HTTPS (Meta/Instagram, TikTok…), **no metas secretos en el binario**. Propón una solución (un backend mínimo de intercambio de tokens o un deep link `omnyra://`) y consúltame antes de implementarla.
- Los tokens se guardan **solo en el llavero del sistema**, nunca en texto plano, en SQLite ni en logs.
- Las credenciales de desarrollo (client IDs) van en un `.env` que está en `.gitignore`. Incluye un `.env.example`.

## 5. Funcionalidades

### 5.1 Vista global (Dashboard)

- **Resumen unificado:** seguidores totales, visitas, engagement y crecimiento del periodo, sumando todas las redes, con filtro por red y por rango de fechas (7 d, 28 d, 90 d, 1 año, personalizado).
- **Gráficas de evolución** de seguidores y visitas, con una línea por red.
- **Top contenidos** de todas las redes, ordenables por visitas, engagement o crecimiento.
- **Comparativa cross-platform:** el mismo contenido publicado en varias redes (por ejemplo, el mismo clip en Shorts, TikTok y Reels) comparado lado a lado. El usuario puede **vincular manualmente** publicaciones como "el mismo contenido".
- **Mejor hora para publicar**, calculada con los **datos propios** del usuario (rendimiento por día de la semana y hora), por red y en global, mostrada como mapa de calor.
- **Alertas:** contenido que rinde muy por encima o por debajo de la media del canal, hitos de seguidores, caída de engagement.
- **Rentabilidad por red:** crecimiento conseguido frente a contenidos publicados en cada red.

### 5.2 Vista individual por red social

Cada red tiene su propia pestaña o sección con:

- **Analíticas específicas** de esa plataforma. Ejemplos:
  - YouTube: retención media, CTR de miniaturas, fuentes de tráfico, Shorts frente a vídeos largos, suscriptores ganados por vídeo.
  - Instagram: alcance, guardados, compartidos, rendimiento de Reels frente a posts.
  - TikTok: visualizaciones, tiempo medio de visualización, compartidos.
  - Twitch: espectadores medios y pico, horas emitidas, seguidores por directo.
  - X: impresiones, interacciones.
- **Lista de contenidos** de esa red con sus métricas.
- **Subida directa de vídeos** (donde la API lo permita), con:
  - Selección de archivo y validación de formato, duración, peso y relación de aspecto para esa red.
  - Título, descripción, etiquetas/hashtags, visibilidad, miniatura, categoría, lista de reproducción, etc., según la red.
  - **Vista previa** de cómo quedará en la plataforma.
  - Barra de progreso, **subida reanudable** si la API lo admite, y cola de subidas.
  - Programación de la publicación (si la API lo permite) o recordatorio local.
- **Comentarios de esa red** (ver 5.5).
- **Calendario filtrado** por esa red.

### 5.3 Publicación multi-red

- Desde la vista global: **subir un vídeo una vez y elegir en qué redes publicarlo**.
- Omnyra genera las versiones necesarias con FFmpeg (16:9, 9:16, 1:1, recorte o relleno, límite de duración) y adapta el texto a cada red (longitud máxima, hashtags, menciones).
- El usuario puede **retocar cada versión** en el apartado de cada red antes de publicar.
- Para redes sin subida por API: la opción **"Preparar y descargar"** exporta el vídeo ya adaptado y copia el texto al portapapeles.

### 5.4 Planificación de contenido

- **Calendario común** (vista de mes y de semana) con todas las publicaciones pasadas y programadas, coloreadas por red, con arrastrar y soltar para reprogramar.
- **Tablero de ideas (Kanban):** Idea → Guion → Grabado → Editado → Publicado. Cada tarjeta puede tener notas, archivos asociados, redes de destino y fecha.
- **Notificaciones del sistema** para publicaciones programadas y recordatorios.

### 5.5 Comunidad: bandeja de comentarios unificada

- Comentarios de todas las redes en una sola bandeja, con filtro por red, por contenido y por estado (pendiente, respondido, archivado).
- **Clasificación automática:** preguntas, spam, negativos y positivos (empezar con reglas y heurísticas; dejar preparado para un modelo más avanzado).
- **Detección de preguntas repetidas** para sacar ideas de contenido, con un botón para "enviar al tablero de ideas".
- Responder desde Omnyra donde la API lo permita.

### 5.6 Herramientas de creación

- **Generador de miniaturas:**
  - Editor sobre canvas con capas: fondo, imagen, texto con contorno y sombra, formas y stickers.
  - **Plantillas** guardables.
  - **Biblioteca de recursos del creador:** el usuario puede subir sus avatares, personajes o skins (por ejemplo, las skins de Minecraft del canal gh0stymc: normal, Halloween y Navidad) y reutilizarlos.
  - Exportación en los tamaños de cada red (1280×720 para YouTube, 1080×1920 para formato vertical, etc.).
- **Asistente de texto:** generador de títulos, descripciones y hashtags adaptados a cada red. La primera versión se basa en plantillas; deja la integración con un LLM como opcional y configurable con la clave del propio usuario.
- **Detector de momentos para shorts:** en vídeos largos o directos, propone fragmentos a partir de los picos de retención (YouTube) o de actividad del chat (Twitch), y los recorta con FFmpeg.

### 5.7 Competencia y tendencias

- Seguimiento de **canales o cuentas públicas** similares: su ritmo de publicación, sus contenidos que mejor funcionan y su crecimiento (solo con datos públicos y respetando los términos de cada API).
- **Tendencias del nicho** a partir de palabras clave configurables.

### 5.8 Monetización

- **Gestor de patrocinios:** marca, contacto, entregables, fechas, importe, estado del pago y archivos del contrato.
- **Media kit automático:** genera una página (HTML) o un PDF con las estadísticas actualizadas del creador, listo para enviar a marcas. Las métricas que se incluyen son configurables.

### 5.9 Módulo de seguridad (diferenciador)

- **Auditoría de cuentas:** checklist por red (2FA activado, correo de recuperación, apps de terceros conectadas) con enlaces directos a la configuración de cada red. Donde la API no dé el dato, el usuario lo marca manualmente.
- **Detección de phishing en comentarios:** enlaces sospechosos, dominios que imitan a otros (typosquatting), estafas típicas tipo "has ganado un sorteo, escríbeme por Telegram"…
- **Detección de suplantación:** buscar cuentas con nombre o avatar parecido al del creador, o que resuban su contenido (dentro de lo que permitan las APIs).
- **Seguridad de la propia app:** tokens en el llavero, permisos OAuth mínimos (pide solo los *scopes* necesarios), opción de bloquear la app con contraseña o PIN, y un registro local de acciones sensibles (publicaciones y conexiones de cuentas).

### 5.10 Ajustes

- Gestión de cuentas conectadas (conectar, desconectar, ver los permisos concedidos).
- Tema claro y oscuro, idioma y zona horaria.
- Comportamiento en segundo plano (bandeja del sistema, frecuencia de los snapshots).
- Exportar e importar datos (backup de SQLite y CSV de métricas).
- Ruta de FFmpeg.

## 6. Diseño y UX

- Estética moderna, con tema oscuro por defecto y una identidad propia de **Omnyra** (genera un logotipo sencillo y una paleta; cada red usa su color de marca solo como acento).
- Barra lateral: **Global**, una entrada por cada red conectada, **Calendario**, **Ideas**, **Comentarios**, **Creación**, **Patrocinios**, **Seguridad** y **Ajustes**.
- Estados vacíos útiles (por ejemplo: "Conecta tu primera red para empezar").
- Debe funcionar sin conexión con los datos ya guardados, y mostrar claramente cuándo fue la última sincronización.

## 7. Limitaciones conocidas de las APIs (verificar)

> ⚠️ Las condiciones de las APIs cambian a menudo. **Consulta la documentación oficial actual de cada plataforma antes de implementar su conector** y avísame si algo de esta tabla ha cambiado.

| Red | Analíticas | Subida por API | Notas |
|---|---|---|---|
| YouTube | Sí (Data API + Analytics API) | Sí | Sin auditoría de Google, los vídeos subidos quedan en privado; la cuota diaria es limitada (vigilar el coste en unidades). |
| TikTok | Limitadas | Sí (Content Posting API) | Sin auditoría, solo publica en privado. |
| Instagram | Sí (Graph API) | Sí (Reels/posts) | Requiere cuenta profesional y app revisada por Meta. |
| X | Según el plan | Sí | La API gratuita está muy limitada. |
| Twitch | Sí | **No** (no hay subida de VOD por API) | Solo analíticas, directos y clips. |

## 8. Modelos de Claude y protocolo de cambio de modelo

### 8.1 Modelos que se usan en el proyecto

| Modelo | ID completo | Cuándo se usa |
|---|---|---|
| **Claude Fable 5.2** | `claude-fable-5-2` | Solo para decisiones de arquitectura y revisiones globales. Es el más potente y el que más tokens gasta. |
| **Claude Opus 5.5** | `claude-opus-5-5` | Implementación compleja: OAuth, seguridad, subidas, algoritmos, FFmpeg, editor de miniaturas, CI y empaquetado. |
| **Claude Sonnet 5.5** | `claude-sonnet-5-5` | El trabajo del día a día: pantallas, componentes, CRUD, tests sencillos. |
| **Claude Haiku 5.5** | `claude-haiku-5-5` | Tareas pequeñas: README, CHANGELOG, traducciones, renombrados, arreglos triviales. |
| **opusplan** | alias `opusplan` | Modo mixto: planifica con Opus y ejecuta con Sonnet. |

Nota: Fable puede cambiar automáticamente a otro modelo cuando una petición entra en temas de ciberseguridad. Por eso **el módulo de seguridad (paso 6c) se hace con Opus, no con Fable**.

### 8.2 Protocolo de cambio de modelo (OBLIGATORIO)

Claude Code **no puede cambiar el modelo de la sesión principal por sí mismo**: eso lo hace el usuario con `/model`. Sí puede delegar tareas a **subagentes** que tengan un modelo fijo. Por eso se trabaja así:

1. **Al empezar cualquier paso**, lee `PROGRESS.md` para saber en qué punto está el proyecto y comprueba qué modelo estás usando (aparece en tu contexto de sistema).
2. **Si el modelo actual coincide** con el que indica el paso, continúa.
3. **Si NO coincide**, **no escribas código**. Detente y muestra exactamente este bloque, rellenado:

```
⏸️ CAMBIO DE MODELO NECESARIO

Siguiente paso:    <número y nombre del paso>
Modelo actual:     <tu modelo>
Modelo necesario:  <ID del modelo>

Qué tienes que hacer:
  1. /clear            (empezar con una sesión limpia ahorra tokens)
  2. /model <ID del modelo>
  3. Escribe: "Continúa con el paso <número> de OMNYRA_SPEC.md"

Antes de continuar, necesito que hagas tú:
  - <tareas manuales del usuario para este paso, o "Nada">
```

4. **Excepción, pasos con subagente:** si un paso indica *"subagente"*, no pidas cambio de modelo: delega la tarea al subagente correspondiente (ver 8.3), que ya tiene su modelo fijado.
5. Si el usuario decide seguir con otro modelo, respétalo, pero avísale una vez del posible impacto (más gasto de tokens o menos calidad).
6. **Al terminar cada paso:**
   - Ejecuta los tests y comprueba que compila.
   - Haz un commit descriptivo.
   - Marca el paso como hecho en `PROGRESS.md`.
   - Muestra el bloque de cambio de modelo **del siguiente paso** (aunque el modelo sea el mismo, indica *"Mismo modelo: solo /clear y continúa"*), junto con las tareas manuales que tenga.

### 8.3 Subagentes con modelo fijo

En el paso 0b se crean estos subagentes en `.claude/agents/`, con el campo `model` en su frontmatter:

| Subagente | Modelo | Para qué |
|---|---|---|
| `docs-writer` | `haiku` (Haiku 5.5) | README, CHANGELOG, comentarios de código, documentación |
| `i18n-translator` | `haiku` (Haiku 5.5) | Traducir y revisar las cadenas ES/EN |
| `test-writer` | `sonnet` (Sonnet 5.5) | Escribir tests para lógica ya implementada |
| `ui-builder` | `sonnet` (Sonnet 5.5) | Pantallas y componentes sencillos cuando la sesión principal está en Opus |

La sesión principal puede delegarles trabajo **sin que el usuario cambie de modelo**. Úsalos solo para tareas acotadas: un subagente empieza sin contexto y tiene que leer los archivos que necesite.

### 8.4 Archivo `PROGRESS.md`

Créalo en el paso 0b y mantenlo siempre actualizado. Contiene:
- La lista de todos los pasos de la sección 9 con casillas `[ ]` / `[x]`.
- El paso actual.
- Las decisiones importantes tomadas (con fecha).
- Los problemas pendientes.

Así, cualquier sesión nueva (después de un `/clear`) sabe dónde se quedó el proyecto.

## 9. Hoja de ruta paso a paso

Cada paso indica: **modelo**, **qué tiene que hacer el usuario antes** y **qué se hace, en orden**. No pases a la fase siguiente sin la confirmación del usuario.

---

### FASE 0 — Preparación y base del proyecto

#### Paso 0.0 — Preparación del entorno (lo hace el usuario, sin Claude Code)
- Instalar **Node.js LTS**, **Rust** (rustup) y **Git**.
- Instalar los requisitos de Tauri 2:
  - **Windows:** Microsoft C++ Build Tools y WebView2 (viene con Windows 11).
  - **Debian/Ubuntu:** `libwebkit2gtk-4.1-dev`, `build-essential`, `libssl-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libsecret-1-dev`.
  - **Arch:** `webkit2gtk-4.1`, `base-devel`, `openssl`, `libayatana-appindicator`, `librsvg`, `libsecret`.
- Instalar **FFmpeg**.
- Crear el repositorio `omnyra` en GitHub y clonarlo.
- Copiar este archivo a la raíz del repo.
- Ejecutar `claude update` para tener la última versión de Claude Code.

#### Paso 0a — Arquitectura · `claude-fable-5-2` · modo plan
**Usuario antes:** nada.
1. Primero, lee este documento entero y señala contradicciones, riesgos o huecos.
2. Después, diseña la arquitectura: el sistema de conectores (interfaz y capacidades), qué lógica va en Rust y cuál en el frontend, y cómo se comunican (comandos y eventos de Tauri).
3. Diseña el esquema de SQLite: cuentas, contenidos, snapshots, publicaciones, ideas, comentarios, patrocinios y recursos.
4. Define la estructura de carpetas del proyecto.
5. Escribe todo en `docs/ARCHITECTURE.md` y espera la aprobación del usuario.
6. Sin escribir código de la app todavía.

#### Paso 0b — Base del proyecto · `claude-sonnet-5-5`
**Usuario antes:** haber aprobado `docs/ARCHITECTURE.md`.
1. Primero, crea el proyecto Tauri 2 con React, TypeScript, Vite y Tailwind.
2. Configura el linter, el formateador, Vitest y `cargo test`.
3. Crea `PROGRESS.md` (ver 8.4) y los subagentes de `.claude/agents/` (ver 8.3).
4. Crea el layout: barra lateral, tema oscuro y claro, enrutado entre secciones.
5. Configura i18n con español por defecto e inglés.
6. Monta SQLite con el sistema de migraciones y el esquema del paso 0a.
7. Implementa la interfaz de conectores y un **conector de prueba (mock)** con datos falsos.
8. Comprueba que `npm run tauri dev` arranca la app.

#### Paso 0c — CI multiplataforma · `claude-opus-5-5`
**Usuario antes:** nada.
1. Primero, crea un workflow de GitHub Actions que compile y pase los tests en Windows y en Ubuntu.
2. Después, genera los artefactos: `.msi` y `.exe` (Windows), `.deb` y AppImage (Linux).
3. Crea el `PKGBUILD` para Arch y un job que lo valide en un contenedor de Arch.
4. Sube los artefactos como *artifacts* del workflow.
5. Delega al subagente `docs-writer` las instrucciones de compilación en el README.

**✅ Fin de la Fase 0:** la app vacía con datos de prueba se instala en Windows, Debian y Arch.

---

### FASE 1 — MVP con YouTube

#### Paso 1a — OAuth de Google y almacenamiento seguro · `claude-opus-5-5`
**Usuario antes:**
- Crear un proyecto en Google Cloud Console.
- Activar la **YouTube Data API v3** y la **YouTube Analytics API**.
- Configurar la pantalla de consentimiento OAuth (modo "Testing", añadiendo su propia cuenta como usuario de prueba).
- Crear una credencial OAuth de tipo **"App de escritorio"** y poner el client ID en `.env`.

Pasos:
1. Primero, implementa el flujo OAuth 2.0 con PKCE y redirección a loopback (puerto aleatorio).
2. Guarda y lee los tokens en el llavero del sistema (crate `keyring`). Prueba en Windows y en Linux.
3. Implementa el refresco automático del token y la desconexión (con revocación del token).
4. Crea la pantalla "Cuentas conectadas" en Ajustes.
5. Escribe tests del flujo con mocks.

#### Paso 1b — Conector de YouTube y snapshots · `opusplan`
**Usuario antes:** nada.
1. Primero, implementa el conector: perfil, métricas del canal, lista de vídeos y métricas por vídeo.
2. Controla el consumo de cuota (unidades por llamada) y guarda en caché lo que no cambie.
3. Implementa los snapshots diarios en SQLite y el relleno de huecos con la Analytics API.
4. Añade la sincronización manual (botón) y la automática al abrir la app.

#### Paso 1c — Mejor hora para publicar · `claude-opus-5-5`
**Usuario antes:** nada.
1. Primero, diseña el algoritmo: rendimiento normalizado de cada vídeo según el día y la hora de publicación, ponderando su antigüedad.
2. Contempla el caso de pocos datos (mostrar un aviso de "datos insuficientes").
3. Implementa el mapa de calor (día × hora).
4. Delega al subagente `test-writer` los tests del algoritmo.

#### Paso 1d — Subida de vídeos a YouTube · `claude-opus-5-5`
**Usuario antes:** tener un vídeo de prueba corto.
1. Primero, implementa la subida reanudable (resumable upload) con barra de progreso.
2. Crea el formulario de metadatos: título, descripción, etiquetas, visibilidad, categoría, lista de reproducción y miniatura.
3. Valida el archivo antes de subirlo (formato, peso y duración).
4. Implementa la cola de subidas y los reintentos si se corta la conexión.
5. Avisa en la interfaz de que, sin la auditoría de Google, los vídeos se suben en privado.

#### Paso 1e — Pantallas del MVP · `claude-sonnet-5-5`
**Usuario antes:** nada.
1. Primero, crea la **vista global**: tarjetas de resumen, gráficas de evolución, top contenidos y selector de rango de fechas.
2. Crea la **vista de YouTube**: analíticas específicas, lista de vídeos y botón "Subir vídeo".
3. Añade estados vacíos, de carga y de error, y el indicador de "última sincronización".
4. Delega al subagente `i18n-translator` la revisión de las traducciones.

**✅ Hito: MVP usable con el canal gh0stymc.** El usuario lo prueba unos días antes de seguir.

---

### FASE 2 — Twitch y planificación

#### Paso 2a — Conector de Twitch · `claude-opus-5-5`
**Usuario antes:** registrar una app en la Twitch Developer Console y poner el client ID en `.env`.
1. Primero, implementa el OAuth de Twitch (reutilizando la infraestructura del paso 1a).
2. Implementa el conector: seguidores, directos, espectadores medios y pico, horas emitidas y clips.
3. Declara `upload: false` en sus capacidades, para que la interfaz no muestre la subida.
4. Crea la vista individual de Twitch.

#### Paso 2b — Calendario y tablero de ideas · `claude-sonnet-5-5`
**Usuario antes:** nada.
1. Primero, crea el calendario (vistas de mes y de semana) con las publicaciones pasadas y programadas, coloreadas por red.
2. Añade arrastrar y soltar para reprogramar.
3. Crea el tablero Kanban: Idea → Guion → Grabado → Editado → Publicado, con tarjetas que admitan notas, archivos y redes de destino.

#### Paso 2c — Segundo plano y notificaciones · `claude-sonnet-5-5`
**Usuario antes:** nada.
1. Primero, añade el icono en la bandeja del sistema y la opción "seguir en segundo plano al cerrar".
2. Programa los snapshots periódicos en segundo plano.
3. Implementa las notificaciones del sistema (publicaciones programadas, alertas de rendimiento, hitos).
4. Prueba en Windows y en Linux (GNOME y KDE).

---

### FASE 3 — Instagram, TikTok y publicación multi-red

#### Paso 3a — Estrategia de autenticación para Meta y TikTok · `claude-fable-5-2` · modo plan
**Usuario antes:** nada.
1. Primero, consulta la documentación actual de Meta (Instagram Graph API) y de TikTok (Login Kit y Content Posting API).
2. Compara las opciones para no incluir el client secret en la app: un backend mínimo de intercambio de tokens (y dónde alojarlo gratis), un deep link `omnyra://` u otras.
3. Escribe la decisión recomendada en `docs/AUTH_STRATEGY.md`, con sus pros, contras y costes.
4. Espera la aprobación del usuario.

#### Paso 3b — Conectores de Instagram y TikTok · `claude-opus-5-5`
**Usuario antes:**
- Crear la app en Meta for Developers y pasar la cuenta de Instagram a profesional (creador).
- Crear la app en TikTok for Developers.
- Poner las credenciales donde indique `docs/AUTH_STRATEGY.md`.

Pasos:
1. Primero, implementa la autenticación según la estrategia aprobada.
2. Implementa el conector de Instagram: métricas, Reels y posts, y publicación.
3. Implementa el conector de TikTok: métricas y publicación (avisando de que, sin auditoría, solo publica en privado).
4. Crea las vistas individuales de las dos redes.

#### Paso 3c — Publicación multi-red · `claude-opus-5-5`
**Usuario antes:** nada.
1. Primero, crea el pipeline de FFmpeg: generar versiones 16:9, 9:16 y 1:1 (recorte o relleno), recortar la duración y comprimir según los límites de cada red.
2. Crea el asistente "Publicar en varias redes": subir el vídeo una vez y elegir las redes.
3. Permite retocar cada versión (texto y formato) antes de publicarla.
4. Implementa "Preparar y descargar" para las redes sin subida por API.
5. Delega al subagente `test-writer` los tests del adaptador de formatos.

---

### FASE 4 — Comunidad

#### Paso 4a — Bandeja de comentarios unificada · `opusplan`
**Usuario antes:** nada.
1. Primero, crea el modelo unificado de comentario y su sincronización desde cada conector.
2. Crea la bandeja con filtros por red, contenido y estado.
3. Implementa las respuestas desde Omnyra donde la API lo permita.

#### Paso 4b — Clasificación y preguntas repetidas · `claude-opus-5-5`
**Usuario antes:** nada.
1. Primero, implementa la clasificación por reglas: preguntas, spam, negativos y positivos.
2. Agrupa las preguntas parecidas (por similitud de texto).
3. Añade el botón "Enviar al tablero de ideas".
4. Escribe tests con comentarios de ejemplo.

---

### FASE 5 — Herramientas de creación

#### Paso 5a — Editor de miniaturas · `claude-opus-5-5`
**Usuario antes:** tener a mano las 3 skins de gh0stymc (normal, Halloween y Navidad) y algunas imágenes de prueba.
1. Primero, crea el editor de canvas con capas: fondo, imágenes, texto con contorno y sombra, formas y stickers.
2. Crea la biblioteca de recursos del creador (subir y reutilizar avatares y skins).
3. Añade las plantillas guardables.
4. Implementa la exportación en los tamaños de cada red e intégrala con la subida de vídeos.

#### Paso 5b — Asistente de textos · `claude-sonnet-5-5`
**Usuario antes:** nada.
1. Primero, crea el generador de títulos, descripciones y hashtags basado en plantillas, por red.
2. Añade la integración opcional con un LLM usando la clave de API del usuario, guardada en el llavero.

#### Paso 5c — Detector de momentos para shorts · `claude-opus-5-5`
**Usuario antes:** tener un vídeo largo o un directo de prueba.
1. Primero, obtén las curvas de retención (YouTube) y la actividad del chat (Twitch).
2. Detecta los picos y propón fragmentos.
3. Recorta los fragmentos en vertical con FFmpeg y envíalos a la publicación multi-red.

---

### FASE 6 — Monetización, competencia y seguridad

#### Paso 6a — Patrocinios y media kit · `claude-sonnet-5-5`
**Usuario antes:** nada.
1. Primero, crea el gestor de patrocinios: marca, contacto, entregables, fechas, importe, estado del pago y archivos.
2. Crea el generador de media kit (HTML y PDF) con las métricas configurables.

#### Paso 6b — Competencia y tendencias · `claude-sonnet-5-5`
**Usuario antes:** pensar qué canales quiere seguir.
1. Primero, permite añadir canales públicos para seguirlos y guardar sus snapshots.
2. Crea las comparativas de crecimiento, ritmo de publicación y top contenidos.
3. Añade las tendencias por palabras clave.

#### Paso 6c — Módulo de seguridad · `claude-opus-5-5` (NO Fable)
**Usuario antes:** nada.
1. Primero, crea la auditoría de cuentas: el checklist por red con enlaces a la configuración de cada una.
2. Implementa la detección de phishing en comentarios: análisis de URLs, dominios parecidos y patrones de estafa.
3. Implementa la detección de suplantación: cuentas con nombre o avatar parecidos, dentro de lo que permitan las APIs.
4. Añade el bloqueo de la app con contraseña o PIN y el registro local de acciones sensibles.
5. Escribe tests exhaustivos de la detección de phishing.

---

### FASE 7 — Revisión final y distribución

#### Paso 7a — Revisión global · `claude-fable-5-2` · modo plan
**Usuario antes:** nada.
1. Revisa todo el código: arquitectura, rendimiento, deuda técnica y consistencia.
2. Escribe en `docs/REVIEW.md` una lista priorizada de mejoras.
3. No hagas cambios: las mejoras se aplican en el paso 7b.
4. Si Fable cambia de modelo automáticamente al revisar partes de seguridad, sigue con el modelo nuevo y anótalo.

#### Paso 7b — Aplicar mejoras · `claude-opus-5-5`
**Usuario antes:** marcar qué mejoras de `docs/REVIEW.md` se aplican.
1. Aplica las mejoras aprobadas, empezando por las de prioridad alta.

#### Paso 7c — Distribución · `claude-opus-5-5`
**Usuario antes:**
- Generar el par de claves de firma del updater de Tauri y guardarlo como secreto en GitHub.
- (Opcional) Crear una cuenta en AUR.

Pasos:
1. Primero, pule los iconos, los metadatos y los instaladores de cada sistema.
2. Configura las auto-actualizaciones firmadas.
3. Crea el workflow de releases que publica todos los artefactos al crear un tag `v*`.
4. Deja el `PKGBUILD` listo para publicarlo en AUR.

#### Paso 7d — Documentación final · `claude-haiku-5-5`
**Usuario antes:** nada.
1. Actualiza el README (instalación en Windows, Debian y Arch, y capturas de pantalla).
2. Escribe el CHANGELOG de la v1.0.0.
3. Escribe una guía de usuario breve en `docs/USER_GUIDE.md`.

**🚀 v1.0.0 publicada.**

## 10. Normas de trabajo para Claude Code

- **Sigue siempre el protocolo de cambio de modelo (8.2)** antes de empezar y al terminar cada paso.
- **Un paso por sesión.** Al terminar un paso, recomienda `/clear`.
- **Pregunta antes de** tomar decisiones de arquitectura grandes, añadir dependencias pesadas o cambiar el stack.
- **Nunca** subas secretos, tokens ni client secrets al repositorio.
- Pide solo los *scopes* OAuth mínimos necesarios para cada funcionalidad.
- Gestiona los errores de las APIs (cuotas agotadas, tokens caducados, límites de uso) con mensajes claros al usuario y reintentos con *backoff*.
- Código en inglés (nombres de variables, funciones y commits); interfaz en español e inglés.
- Escribe tests para la lógica de negocio (cálculo de la mejor hora, agregación de métricas, adaptación de formatos, detección de phishing).
- Mantén `README.md`, `CHANGELOG.md` y `PROGRESS.md` actualizados (delegando en `docs-writer` cuando sea posible).
- Cuando algo de este documento no se pueda hacer por limitaciones de una API, dímelo y propón la mejor alternativa en lugar de saltártelo en silencio.
- Si una tarea con Sonnet falla dos veces seguidas, propón pasar a Opus. Propón Fable solo si Opus tampoco la saca adelante.
