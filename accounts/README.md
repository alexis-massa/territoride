# accounts

Strava-only auth. There is no local signup/login — identity is a player's
Strava `athlete_id`.

**Files**
- `models.py` — `User(AbstractUser)`: adds `athlete_id` and Strava tokens,
  exposed as `strava_access_token`/`strava_refresh_token` properties that
  transparently encrypt/decrypt the backing `..._encrypted` fields. Players
  never get a usable password; only a manually created superuser
  (`createsuperuser`) has one, for `/admin/` access.
- `crypto.py` — `encrypt_token`/`decrypt_token`, Fernet-based
  (`settings.TOKEN_ENCRYPTION_KEY`).
- `views.py` — the two-step OAuth handshake (`strava_authorize`,
  `strava_callback`).
- `urls.py` — `/accounts/strava/authorize/`, `/accounts/strava/callback/`,
  `/accounts/logout/` (Django's built-in `LogoutView`).
- `templates/accounts/_topbar.html` — included on every page; shows
  "Connect with Strava" or the username + a log-out form depending on
  `user.is_authenticated`.
- `templates/accounts/oauth_complete.html` — the page the popup lands on
  after the callback; posts the result to the opener window and closes
  itself.

**Flow**

1. "Connect with Strava" button (in the shared topbar) opens a popup on
   `strava_authorize`.
2. `strava_authorize` redirects the popup to Strava's real login page
   (CSRF `state` stored in the session).
3. After the user logs in on Strava, it redirects back to
   `strava_callback` with a `code`.
4. `strava_callback` exchanges the code for tokens, `get_or_create`s the
   `User` by `athlete_id`, saves the (encrypted) tokens, and logs them in.
5. The popup renders `oauth_complete.html`, which `postMessage`s the
   result to the main window and closes itself; the main window reloads.

No approval step — anyone completing Strava OAuth is in immediately.
