# TuffSwarm Supabase backend

Preferred start transport for signed `ExperienceCapsule` exchange.

## Deploy

```bash
# From repo root, with Supabase CLI linked to your project:
supabase db push
supabase functions deploy publish-capsule --no-verify-jwt
supabase functions deploy vote-capsule
supabase functions deploy report-cooccurrence --no-verify-jwt
```

`vote-capsule` must keep JWT verification on (Auth login required to vote).
`report-cooccurrence` is anon-callable (rate-limited; service role inside).

## Client settings

TuffBox ships with the community Supabase URL + publishable key built in.
Users only enable **Use TuffSwarm network** — no keys to paste.

Optional Advanced override in Settings for self-hosted projects.

**Never** put the service role or personal access token in the client.

## Security model

| Layer | Behavior |
|-------|----------|
| RLS | `experience_capsules`: SELECT for anon/authenticated on `open`/`saved` only; `rejected` hidden |
| RLS | `mod_cooccurrence_pairs`: SELECT for anon/authenticated; writes via Edge Function only |
| Write path | Edge Function `publish-capsule` with service role (`verify_jwt=false`; Ed25519 soft-sign) |
| Co-occurrence | Edge Function `report-cooccurrence` expands mod sets → pair counts (`bump_mod_cooccurrence_pairs`) |
| Capsule | Must include `contentHash` + Ed25519 signature + ≥1 valid action |
| Client counters | **Ignored** — new capsules start `open`, `success_count=0` |
| Peer votes | `vote-capsule` requires Supabase Auth JWT (`verify_jwt=true`); one vote per user; 2 Keep → `saved`; 3 Discard → `rejected` |
| Crash Votes UI | Register / sign in required before Keep/Discard |
| Rate limits | Per signer / hour, per fingerprint / day, open-per-signer+fp cap; co-occurrence 30 reports/hour/device |
| Privacy | Reject notes / raw crash logs |

**Never** put the service role or personal access token in the client.
