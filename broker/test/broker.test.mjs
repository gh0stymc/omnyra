import { test } from "node:test";
import assert from "node:assert/strict";
import { handle, loopbackTarget } from "../src/index.js";

const env = {
  INSTAGRAM_CLIENT_ID: "ig-id",
  INSTAGRAM_CLIENT_SECRET: "ig-secret",
  TIKTOK_CLIENT_KEY: "tt-key",
  TIKTOK_CLIENT_SECRET: "tt-secret",
};
const form = (o) =>
  new Request("https://b.example/x", {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams(o),
  });
const post = (path, o) => new Request(`https://b.example${path}`, form(o));

test("loopback target only accepts numeric ports and always targets 127.0.0.1", () => {
  assert.equal(loopbackTarget("5555.abcdefghijklmnopqrst"), "http://127.0.0.1:5555/");
  assert.equal(loopbackTarget("evil.com.abcdefghijklmnopqrst"), null);
  assert.equal(loopbackTarget("80.abcdefghijklmnopqrst"), null);
  assert.equal(loopbackTarget("5555.short"), null);
  assert.equal(loopbackTarget(null), null);
});

test("instagram callback redirects to the loopback with the code", async () => {
  const r = await handle(
    new Request(
      "https://b.example/instagram/callback?code=abc%23_&state=5555.abcdefghijklmnopqrst",
    ),
    env,
  );
  assert.equal(r.status, 302);
  const loc = new URL(r.headers.get("location"));
  assert.equal(loc.host, "127.0.0.1:5555");
  assert.equal(loc.searchParams.get("code"), "abc");
  assert.equal(loc.searchParams.get("state"), "5555.abcdefghijklmnopqrst");
});

test("instagram code exchange adds the secret and returns a long-lived token", async () => {
  const calls = [];
  const fake = async (url, init) => {
    calls.push({ url: String(url), body: init?.body?.toString() });
    if (String(url).startsWith("https://api.instagram.com"))
      return Response.json({ data: [{ access_token: "short", permissions: "a,b" }] });
    return Response.json({ access_token: "long", expires_in: 5184000 });
  };
  const r = await handle(
    post("/instagram/token", {
      client_id: "ig-id",
      grant_type: "authorization_code",
      code: "c",
      redirect_uri: "https://b.example/instagram/callback",
    }),
    env,
    fake,
  );
  const body = await r.json();
  assert.equal(body.access_token, "long");
  assert.equal(body.refresh_token, "long");
  assert.equal(body.scope, "a b");
  assert.match(calls[0].body, /client_secret=ig-secret/);
  assert.match(calls[1].url, /ig_exchange_token/);
});

test("rejects a foreign client id", async () => {
  const r = await handle(
    post("/tiktok/token", { client_id: "other", grant_type: "refresh_token" }),
    env,
    fetch,
  );
  assert.equal(r.status, 401);
});

test("tiktok refresh forwards with the secret and normalises scopes", async () => {
  let sent;
  const fake = async (_u, init) => {
    sent = init.body.toString();
    return Response.json({
      access_token: "a",
      refresh_token: "r",
      expires_in: 86400,
      scope: "user.info.basic,video.list",
    });
  };
  const r = await handle(
    post("/tiktok/token", {
      client_id: "tt-key",
      grant_type: "refresh_token",
      refresh_token: "old",
    }),
    env,
    fake,
  );
  const body = await r.json();
  assert.equal(body.scope, "user.info.basic video.list");
  assert.match(sent, /client_secret=tt-secret/);
  assert.match(sent, /refresh_token=old/);
});

test("tiktok error becomes invalid_grant", async () => {
  const fake = async () => Response.json({ error: "invalid_grant" }, { status: 400 });
  const r = await handle(
    post("/tiktok/token", { client_id: "tt-key", grant_type: "refresh_token", refresh_token: "x" }),
    env,
    fake,
  );
  assert.equal((await r.json()).error, "invalid_grant");
});
