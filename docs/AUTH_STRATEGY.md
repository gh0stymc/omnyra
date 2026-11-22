# Omnyra — Estrategia de autenticación para Meta (Instagram) y TikTok (paso 3a)

> Estado: **aplicada** · 2026-11-22 · redactado con `claude-opus-5-5` (el usuario pidió hacer todos los pasos seguidos con Opus en lugar de Fable).
> Lo marcado con **[VERIFICAR]** depende de la documentación actual y se comprobará con las apps reales.

## 1. Qué dicen las APIs (consultado 2026-11-22)

### Instagram — "Instagram API with Instagram Login" (Business Login for Instagram)

- Funciona con cuentas **profesionales** (creador o empresa) sin necesidad de página de Facebook.
- Flujo: `https://www.instagram.com/oauth/authorize` → `code` (válido 1 h) → `POST https://api.instagram.com/oauth/access_token` (**exige `client_secret`**) → token corto (1 h) → `GET https://graph.instagram.com/access_token?grant_type=ig_exchange_token&client_secret=…` → token **largo de 60 días**. Meta indica que el intercambio "se debe hacer en código del servidor".
- Refresco: `GET https://graph.instagram.com/refresh_access_token?grant_type=ig_refresh_token&access_token=…` (sin secreto). Solo funciona con tokens de más de 24 h y no caducados; un token que no se refresca en 60 días muere.
- Los `redirect_uri` válidos tienen que ser **HTTPS** registrados en el panel de Meta [VERIFICAR: no se aceptan `http://127.0.0.1`]. No hay PKCE.
- Permisos: `instagram_business_basic`, `instagram_business_manage_insights`, `instagram_business_content_publish`, `instagram_business_manage_comments`. Sin revisión de la app (modo desarrollo) solo funciona con cuentas añadidas como testers, que es justo el caso de uso personal.
- Publicación de Reels: contenedor `POST /<ig-user-id>/media` (`media_type=REELS`, `upload_type=resumable`) → subida del binario a `https://rupload.facebook.com/ig-api-upload/<versión>/<container-id>` → `POST /<ig-user-id>/media_publish`. No hace falta alojar el vídeo en una URL pública (A6 resuelto).

### TikTok — Login Kit for Desktop + Display API + Content Posting API

- Desktop: `https://www.tiktok.com/v2/auth/authorize/` con **PKCE obligatorio** y redirect a `http://localhost:<puerto>` o `127.0.0.1` (se admite comodín de puerto). El parámetro se llama `client_key`, y los scopes van separados por comas.
- Intercambio: `POST https://open.tiktokapis.com/v2/oauth/token/` con `client_key`, **`client_secret`**, `code`, `code_verifier`, `redirect_uri`. El refresco (`grant_type=refresh_token`) también lleva `client_secret`. Access token de 24 h, refresh token de 365 días.
- [VERIFICAR] En escritorio el `code_challenge` es el SHA-256 del verifier **en hexadecimal**, no en base64url como dice el RFC 7636. Omnyra lo deja configurable (`pkce_hex`).
- Display API (`user.info.basic`, `user.info.stats`, `video.list`): seguidores, likes, nº de vídeos, y por vídeo visualizaciones, likes, comentarios y compartidos. Sin tiempo medio de visualización ni lectura de comentarios (A4 confirmado).
- Content Posting API (`video.publish`): `POST /v2/post/publish/video/init/` con `source=FILE_UPLOAD`, trozos de 5–64 MB (máx. 1000) por `PUT` con `Content-Range` (206 = faltan, 201 = completo). **Clientes sin auditar: solo `SELF_ONLY` (privado), máximo 5 usuarios por día y la cuenta tiene que ser privada al publicar** (si no: `unaudited_client_can_only_post_to_private_accounts`). La auditoría es aparte de la revisión de la app.

**Conclusión:** las dos redes exigen un `client_secret` confidencial en el intercambio (y TikTok también en cada refresco). La regla del spec es no meterlo en el binario.

## 2. Opciones comparadas

| Opción | Cómo | Pros | Contras |
|---|---|---|---|
| **A. Secreto en el binario** | Como Google (R3) | Cero infraestructura | Meta y TikTok sí lo tratan como confidencial: cualquiera lo extrae del ejecutable y puede suplantar la app. **Descartada** por el spec. |
| **B. Deep link `omnyra://`** | Redirect a un esquema propio | Sin servidor para la redirección | No resuelve el secreto (el intercambio sigue necesitándolo). Meta no acepta esquemas propios. Registro del esquema distinto en cada SO. **Descartada.** |
| **C. Token broker mínimo** (recomendada) | Un servidor sin estado guarda los dos secretos y solo hace el intercambio `code → token` y el refresco. Para Instagram, además, sirve el `redirect_uri` HTTPS y reenvía al loopback local. | El secreto nunca sale del servidor. Los tokens nunca se guardan allí (solo pasan). Reutiliza el OAuth+PKCE+loopback que ya existe. | Hay que desplegar y mantener algo (≈150 líneas). Si el broker cae, no se puede conectar ni refrescar TikTok (Instagram se refresca sin él). |
| **D. Que cada usuario cree su app** | El usuario pega su propio secreto, que va al llavero | Sin servidor | Inviable para usuarios normales (verificación de Meta, revisión de TikTok). Útil solo como modo avanzado. |

### Dónde alojar el broker (gratis)

| Servicio | Plan gratuito | Encaje |
|---|---|---|
| **Cloudflare Workers** (recomendado) | 100.000 peticiones/día, HTTPS con dominio `*.workers.dev`, secretos cifrados (`wrangler secret put`) | Perfecto: sin estado, arranque en frío inexistente. |
| Deno Deploy | 1 M peticiones/mes | Igual de válido; el código del Worker funciona casi tal cual. |
| Vercel / Netlify Functions | Generosos | Más pesados para algo tan pequeño. |

Coste: **0 €** al volumen de un creador (unas pocas peticiones al día por cuenta).

## 3. Decisión

**Opción C con Cloudflare Workers**, implementada en `broker/` (Worker en JavaScript, sin dependencias):

```
App (127.0.0.1:<puerto>)          Broker (https://omnyra-broker.<tú>.workers.dev)         Red social
  │ abre navegador ─────────────────────────────────────────────────────────────────▶ authorize
  │                                    ◀── (Instagram) /instagram/callback?code&state ──┘
  │ ◀── 302 http://127.0.0.1:<puerto>/?code&state  (el puerto va dentro de `state`)
  │ POST /instagram/token | /tiktok/token (code, verifier, redirect_uri) ─▶ añade client_secret ─▶ token
  │ ◀── {access_token, refresh_token, expires_in, scope}  (formato OAuth estándar)
```

- **TikTok:** redirect directo al loopback (TikTok lo permite); solo el intercambio y el refresco pasan por el broker.
- **Instagram:** el `redirect_uri` es `https://<broker>/instagram/callback`. El `state` es `<puerto>.<aleatorio>`; el broker solo acepta un puerto numérico y redirige **siempre** a `127.0.0.1`, así que no es un open redirect. El broker hace el intercambio corto→largo y devuelve el token largo también como `refresh_token`; refrescar = `ig_refresh_token` a través del broker (así la app no distingue redes).
- El broker **no guarda nada**, no registra cuerpos y solo acepta su propio `client_id`. Rechaza peticiones con un `client_id` distinto.
- Riesgo residual: alguien podría usar el broker para canjear códigos de *su propio* consentimiento a nombre de la app de Omnyra. No da acceso a cuentas ajenas (hace falta el `code` y, en TikTok, el verifier PKCE). Se mitiga con el límite de peticiones de Cloudflare y, si hiciera falta, con una clave compartida (no secreta) por cabecera.
- **En la app:** `OAuthConfig` gana `client_id_param` (`client_key`), `scope_separator` (`,`), `pkce_hex` y `redirect_broker`. `token_url` apunta al broker y `client_secret` es siempre `None`.

### Credenciales (dónde las pones tú)

| Variable | Dónde | Qué es |
|---|---|---|
| `OMNYRA_BROKER_URL` | `.env` de la app | URL del Worker (`https://omnyra-broker.<tú>.workers.dev`) |
| `OMNYRA_INSTAGRAM_CLIENT_ID` | `.env` de la app | *Instagram App ID* (público) |
| `OMNYRA_TIKTOK_CLIENT_KEY` | `.env` de la app | *Client key* de TikTok (público) |
| `INSTAGRAM_CLIENT_ID`, `INSTAGRAM_CLIENT_SECRET` | `wrangler secret put` | Solo en el broker |
| `TIKTOK_CLIENT_KEY`, `TIKTOK_CLIENT_SECRET` | `wrangler secret put` | Solo en el broker |

Sin `OMNYRA_BROKER_URL` o sin el ID, la red aparece como "no configurada" (igual que YouTube sin client ID).

### Pasos manuales (usuario)

1. **Instagram:** pasar la cuenta a profesional (Ajustes → Tipo de cuenta → Creador). En developers.facebook.com: crear app de tipo *Business* → producto *Instagram* → *API setup with Instagram login* → añadir tu cuenta como *Instagram Tester* y aceptarlo en la app de Instagram → en *Business login settings*, `Valid OAuth Redirect URIs` = `https://<broker>/instagram/callback`.
2. **TikTok:** en developers.tiktok.com crear app → plataforma *Desktop* → redirect `http://127.0.0.1` con puerto comodín → productos *Login Kit*, *Content Posting API* (con *Direct Post*) → scopes `user.info.basic`, `user.info.stats`, `video.list`, `video.publish` → añadir tu cuenta como *target user* del sandbox.
3. **Broker:** `cd broker && npx wrangler login && npx wrangler deploy`, y `npx wrangler secret put …` para los cuatro secretos. Copiar la URL al `.env`.

## 4. Pros, contras y costes de la decisión

- ✅ Ningún secreto confidencial en el binario ni en el repo.
- ✅ Cambio mínimo en la app: el mismo flujo PKCE + loopback.
- ✅ Gratis.
- ⚠️ Una pieza más que desplegar; si se distribuye a terceros, el broker pasa a ser un servicio con usuarios (privacidad: solo ve tokens en tránsito; documentarlo).
- ⚠️ TikTok sin auditar: publicación solo privada y cuenta privada; Instagram sin revisión: solo cuentas tester. Para uso personal basta.
