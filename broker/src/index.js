// Omnyra token broker: holds the Instagram and TikTok client secrets so they never ship in
// the desktop binary. Stateless: tokens only pass through, nothing is stored or logged.
//
//   GET  /instagram/callback?code&state   -> 302 to http://127.0.0.1:<port>/ (port from state)
//   POST /instagram/token                 -> code exchange (short -> long lived) or refresh
//   POST /tiktok/token                    -> code exchange or refresh
//
// Responses use the standard OAuth shape: {access_token, refresh_token, expires_in, scope}.

const IG_AUTH_TOKEN = "https://api.instagram.com/oauth/access_token";
const IG_GRAPH = "https://graph.instagram.com";
const TT_TOKEN = "https://open.tiktokapis.com/v2/oauth/token/";

const json = (body, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
const oauthError = (error, status = 400) => json({ error }, status);

/** `state` is `<port>.<random>`; only a numeric port is accepted, and the target is always loopback. */
export function loopbackTarget(state) {
  const m = /^(\d{2,5})\.[A-Za-z0-9_-]{16,128}$/.exec(state ?? "");
  if (!m) return null;
  const port = Number(m[1]);
  if (port < 1024 || port > 65535) return null;
  return `http://127.0.0.1:${port}/`;
}

async function readForm(request) {
  const type = request.headers.get("content-type") ?? "";
  if (!type.includes("application/x-www-form-urlencoded")) return null;
  return new URLSearchParams(await request.text());
}

function igCallback(url) {
  const state = url.searchParams.get("state");
  const target = loopbackTarget(state);
  if (!target) return new Response("Invalid state", { status: 400 });
  const out = new URL(target);
  for (const k of ["code", "state", "error", "error_reason", "error_description"]) {
    const v = url.searchParams.get(k);
    // Instagram appends "#_" to the code.
    if (v !== null) out.searchParams.set(k, k === "code" ? v.replace(/#_$/, "") : v);
  }
  return Response.redirect(out.toString(), 302);
}

async function igToken(form, env, fetchImpl) {
  if (form.get("client_id") !== env.INSTAGRAM_CLIENT_ID) return oauthError("invalid_client", 401);
  const grant = form.get("grant_type");
  if (grant === "authorization_code") {
    const short = await fetchImpl(IG_AUTH_TOKEN, {
      method: "POST",
      body: new URLSearchParams({
        client_id: env.INSTAGRAM_CLIENT_ID,
        client_secret: env.INSTAGRAM_CLIENT_SECRET,
        grant_type: "authorization_code",
        redirect_uri: form.get("redirect_uri") ?? "",
        code: form.get("code") ?? "",
      }),
    });
    if (!short.ok) return oauthError("invalid_grant");
    const s = await short.json();
    const shortToken = s.access_token ?? s.data?.[0]?.access_token;
    const permissions = s.permissions ?? s.data?.[0]?.permissions ?? [];
    const longUrl = new URL(`${IG_GRAPH}/access_token`);
    longUrl.searchParams.set("grant_type", "ig_exchange_token");
    longUrl.searchParams.set("client_secret", env.INSTAGRAM_CLIENT_SECRET);
    longUrl.searchParams.set("access_token", shortToken);
    const long = await fetchImpl(longUrl.toString());
    if (!long.ok) return oauthError("invalid_grant");
    const l = await long.json();
    const scope = Array.isArray(permissions)
      ? permissions.join(" ")
      : String(permissions).replaceAll(",", " ");
    return json({
      access_token: l.access_token,
      // Instagram has no refresh token: the long-lived token refreshes itself.
      refresh_token: l.access_token,
      expires_in: l.expires_in,
      scope,
    });
  }
  if (grant === "refresh_token") {
    const u = new URL(`${IG_GRAPH}/refresh_access_token`);
    u.searchParams.set("grant_type", "ig_refresh_token");
    u.searchParams.set("access_token", form.get("refresh_token") ?? "");
    const r = await fetchImpl(u.toString());
    if (!r.ok)
      return oauthError(
        r.status >= 500 ? "server_error" : "invalid_grant",
        r.status >= 500 ? 502 : 400,
      );
    const t = await r.json();
    return json({
      access_token: t.access_token,
      refresh_token: t.access_token,
      expires_in: t.expires_in,
    });
  }
  return oauthError("unsupported_grant_type");
}

async function ttToken(form, env, fetchImpl) {
  // The app sends the client key as `client_id` (generic OAuth code) or `client_key`.
  const key = form.get("client_key") ?? form.get("client_id");
  if (key !== env.TIKTOK_CLIENT_KEY) return oauthError("invalid_client", 401);
  const grant = form.get("grant_type");
  const body = new URLSearchParams({
    client_key: env.TIKTOK_CLIENT_KEY,
    client_secret: env.TIKTOK_CLIENT_SECRET,
    grant_type: grant ?? "",
  });
  if (grant === "authorization_code") {
    for (const k of ["code", "code_verifier", "redirect_uri"]) body.set(k, form.get(k) ?? "");
  } else if (grant === "refresh_token") {
    body.set("refresh_token", form.get("refresh_token") ?? "");
  } else {
    return oauthError("unsupported_grant_type");
  }
  const r = await fetchImpl(TT_TOKEN, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body,
  });
  if (r.status >= 500) return oauthError("server_error", 502);
  const t = await r.json();
  if (!r.ok || t.error)
    return oauthError(t.error === "invalid_request" ? "invalid_request" : "invalid_grant");
  return json({
    access_token: t.access_token,
    refresh_token: t.refresh_token,
    expires_in: t.expires_in,
    scope: (t.scope ?? "").replaceAll(",", " "),
    open_id: t.open_id,
  });
}

export async function handle(request, env, fetchImpl = fetch) {
  const url = new URL(request.url);
  if (request.method === "GET" && url.pathname === "/instagram/callback") return igCallback(url);
  if (
    request.method === "POST" &&
    (url.pathname === "/instagram/token" || url.pathname === "/tiktok/token")
  ) {
    const form = await readForm(request);
    if (!form) return oauthError("invalid_request");
    try {
      return url.pathname.startsWith("/instagram")
        ? await igToken(form, env, fetchImpl)
        : await ttToken(form, env, fetchImpl);
    } catch {
      return oauthError("server_error", 502);
    }
  }
  if (url.pathname === "/") return new Response("Omnyra token broker", { status: 200 });
  return new Response("Not found", { status: 404 });
}

export default { fetch: (request, env) => handle(request, env) };
