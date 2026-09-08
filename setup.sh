#!/usr/bin/env bash
# One-shot dev environment setup: generates .env (with real secrets for the
# values that should be random, blank for the ones only Strava can give
# you), wires up the changelog commit hook, then builds and starts the app.
set -euo pipefail
cd "$(dirname "$0")"

if [ -f .env ]; then
  echo ".env already exists - remove it first if you want to regenerate it."
  exit 1
fi

urlsafe_random() { openssl rand -base64 "$1" | tr '+/' '-_'; }

echo "Strava API credentials (from https://www.strava.com/settings/api -"
echo "set that app's 'Authorization Callback Domain' to localhost). Leave"
echo "blank to fill in later - the app will still run without them."
read -rp "  Client ID: " STRAVA_CLIENT_ID
read -rp "  Client Secret: " STRAVA_CLIENT_SECRET

cat > .env << EOF
DEBUG=True
SECRET_KEY=$(openssl rand -base64 48)
ALLOWED_HOSTS=localhost,127.0.0.1

POSTGRES_DB=territoride
POSTGRES_USER=territoride
POSTGRES_PASSWORD=$(openssl rand -base64 24)
POSTGRES_HOST=db
POSTGRES_PORT=5432

STRAVA_CLIENT_ID=${STRAVA_CLIENT_ID}
STRAVA_CLIENT_SECRET=${STRAVA_CLIENT_SECRET}
STRAVA_WEBHOOK_VERIFY_TOKEN=$(urlsafe_random 32)

TOKEN_ENCRYPTION_KEY=$(urlsafe_random 32)
EOF
echo "Wrote .env"

git config core.hooksPath .githooks
echo "Changelog commit hook enabled"

docker compose up -d --build
docker compose exec app uv run python manage.py migrate

echo
echo "Ready: http://localhost:8000"
if [ -z "$STRAVA_CLIENT_ID" ]; then
  echo "Strava login won't work until you fill in STRAVA_CLIENT_ID/SECRET in .env and restart."
fi
