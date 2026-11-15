# Decisions

Choices this app made where pi-kit or the brief left a detail open. Newest first.

| Date | Decision | Why |
| --- | --- | --- |
| 2026-11-15 | Version 0.8.0 for the rewrite | Same game and data, so not a 1.0 break; the Python version is `python-final` (v0.7.0) |
| 2026-11-15 | The Django admin is replaced by CLI commands (`list-users`, `list-activities`, `delete-activity`, `delete-user`, `leaderboard`, `recapture`) | The admin was used for the owner's own chores (release an activity, recapture, delete a player); no web admin means no admin password and no extra attack surface. Add admin pages later only if a need shows up |
| 2026-11-15 | Password login is dropped; players log in through Strava only. Old Django password hashes stay in the table untouched | Players never had passwords; the owner's admin login is gone with the admin. Rolling back to Python keeps working |
| 2026-11-15 | After cutover everyone logs in again through Strava (Django sessions are not read) | Their sessions are a different format; one popup click. Strava tokens and activities are kept |
| 2026-11-15 | The webhook stays at `/accounts/strava/webhook/` and the OAuth callback is `/api/strava/callback` | The existing Strava subscription points at the old webhook path; the callback only needs the same domain, which Strava checks |
| 2026-11-15 | A Strava webhook event is answered at once and handled in a background task; a revoked authorisation (athlete event) clears the tokens | Strava wants an answer within 2 s and the activity fetch can be slower; the Python code ignored athlete events |
| 2026-11-15 | The OAuth state travels in a short-lived `SameSite=Lax` cookie, the session cookie is `Strict` | The state must come back with Strava's cross-site redirect; the session never needs to |
| 2026-11-15 | A new player whose Strava name is taken gets `_<athlete id>` appended | Python would have failed with a database error |
| 2026-11-15 | The daily release runs in the server at `DECAY_HOUR_UTC` (default 4, `-1` = off); the CLI command stays | One fewer cron job on the Pi; running both is harmless (idempotent) |
| 2026-11-15 | MapLibre is bundled (not loaded from unpkg) and its worker is served from our origin; the CSP allows only OpenFreeMap tiles besides self | No third-party script in the page |
| 2026-11-15 | Map tooltips are built from DOM nodes, not HTML strings | Player names come from Strava; the Python page inserted them as HTML |
| 2026-11-15 | No `robots.txt` | The Python version had none |
| 2026-11-15 | Loops of fewer than 3 points fall back to their touched cells (Python raised an error) | Strava can send such tracks; a crash helps nobody |
| 2026-11-15 | The rules page names summits too | The Python text said "mountain passes for now"; summits exist since 0.6 |
