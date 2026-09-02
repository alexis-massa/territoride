# accounts

Strava-only auth. There is no local signup/login — identity is a player's
Strava `athlete_id`.

**Files**
- `models.py` — `User(AbstractUser)`: adds `athlete_id`, encrypted Strava
  tokens, `strava_token_expires_at`. Players never get a usable password;
  only a manually created superuser (`createsuperuser`) has one, for
  `/admin/` access.
- `crypto.py` — `EncryptedTextField`, a descriptor that transparently
  encrypts/decrypts a backing `TextField` with Fernet
  (`settings.TOKEN_ENCRYPTION_KEY`). Used for both Strava tokens.
- `views.py` — the two-step OAuth handshake (`strava_authorize`,
  `strava_callback`).
- `urls.py` — `/accounts/strava/authorize/`, `/accounts/strava/callback/`.
- `templates/accounts/oauth_complete.html` — the page the popup lands on
  after the callback; posts the result to the opener window and closes
  itself.

**Flow**

1. "Connect with Strava" button (`game/templates/game/map.html`) opens a
   popup on `strava_authorize`.
2. `strava_authorize` redirects the popup to Strava's real login page
   (CSRF `state` stored in the session).
3. After the user logs in on Strava, it redirects back to
   `strava_callback` with a `code`.
4. `strava_callback` exchanges the code for tokens, `get_or_create`s the
   `User` by `athlete_id`, saves the (encrypted) tokens, and logs them in.
5. The popup renders `oauth_complete.html`, which `postMessage`s the
   result to the main window and closes itself; the main window reloads.

No approval step — anyone completing Strava OAuth is in immediately.
