#!/usr/bin/env bash
# pi-kit v0.4.1
# Ship an image to the Pi and switch to it; if the health check fails, switch back. Usually run as
# `just deploy` / `just rollback`.
#
#   deploy.sh APP VERSION      deploy the local image APP:VERSION (built by `just image`)
#   deploy.sh --rollback APP   run the image that was current before the last deploy
#
# On the Pi an instance lives in ~/INSTANCE/ (compose.yml from this repo + .env from pi-infra's add-app.sh)
# and is the compose project INSTANCE; its database is backed up as INSTANCE. INSTANCE is the app's name
# unless set: a second instance (a preview beside the live app) has its own folder, .env (database, domain)
# and project, and shares the app's images.
# Images are tagged APP:VERSION, APP:current (running) and APP:previous (rollback target).
# Environment: INSTANCE, PI_HOST (default amassa@raspberrypi.local), HEALTH_TIMEOUT seconds (default 90).
set -euo pipefail

PI_HOST=${PI_HOST:-amassa@raspberrypi.local}
HEALTH_TIMEOUT=${HEALTH_TIMEOUT:-90}

remote() { ssh -o BatchMode=yes "$PI_HOST" "$@"; }

# Point APP:current at a tag on the Pi, (re)start the instance and wait until it is healthy.
switch_to() {
  local app=$1 instance=$2 tag=$3
  remote "APP=$app INSTANCE=$instance TAG=$tag TIMEOUT=$HEALTH_TIMEOUT bash -s" <<'EOF'
set -euo pipefail
cd ~/"$INSTANCE"
compose() { docker compose -p "$INSTANCE" -f compose.yml "$@"; }
docker tag "$APP:$TAG" "$APP:current"
compose up -d --remove-orphans
for _ in $(seq "$TIMEOUT"); do
  status=$(docker inspect -f '{{.State.Health.Status}}' "$(compose ps -q app)" 2>/dev/null || true)
  [ "$status" = healthy ] && exit 0
  sleep 1
done
compose logs --tail 30 app >&2
exit 1
EOF
}

deploy() {
  local app=$1 version=$2 instance=${INSTANCE:-$1} started=$SECONDS
  if [ -n "$(git status --porcelain)" ] && [ "${ALLOW_DIRTY:-}" != 1 ]; then
    echo "the working tree has uncommitted changes; commit them (or ALLOW_DIRTY=1)" >&2
    exit 1
  fi
  remote "test -f ~/$instance/.env" || {
    echo "$instance/.env is missing in the home folder on the Pi: run pi-infra's add-app.sh $instance first" >&2
    exit 1
  }
  echo "==> copying compose.yml and the image $app:$version"
  remote "cat > ~/$instance/compose.yml" <compose.yml
  docker save "$app:$version" | gzip -1 | remote "gunzip | docker load -q"
  echo "==> backing up the database"
  if remote "test -x ~/pi-infra/backup.sh"; then
    remote "\$HOME/pi-infra/backup.sh $instance"
  else
    echo "warning: ~/pi-infra/backup.sh not found, no backup taken" >&2
  fi
  local had_current=0
  if remote "docker image inspect $app:current >/dev/null 2>&1"; then
    had_current=1
    remote "docker tag $app:current $app:previous"
  fi
  echo "==> starting $app:$version as $instance"
  if switch_to "$app" "$instance" "$version"; then
    remote "echo $version > ~/$instance/.deployed"
    echo "==> $instance $version is healthy ($((SECONDS - started)) s)"
    return
  fi
  echo "==> $instance $version is not healthy" >&2
  if [ "$had_current" = 1 ]; then
    echo "==> rolling back to the previous image" >&2
    switch_to "$app" "$instance" previous
  fi
  exit 1
}

rollback() {
  local app=$1 instance=${INSTANCE:-$1}
  remote "docker image inspect $app:previous >/dev/null" || {
    echo "no previous image of $app on the Pi" >&2
    exit 1
  }
  switch_to "$app" "$instance" previous
  echo "==> $instance is back on the previous image"
}

case ${1:-} in
--rollback) rollback "${2:?usage: deploy.sh --rollback APP}" ;;
-h | --help | "") sed -n '2,15p' "$0" ;;
*) deploy "$1" "${2:?usage: deploy.sh APP VERSION}" ;;
esac
