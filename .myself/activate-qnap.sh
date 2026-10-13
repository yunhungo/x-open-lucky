#!/bin/sh
# @created 2026-10-11
# @description 切换 QNAP 镜像，核对版本，失败回滚并限定清理旧镜像。
# @author yunhungo
set -eu
ROOT=$1
SHA=$2
IMAGE=$3
DOCKER=$4
case "$SHA" in *[!a-f0-9]*|'') echo 'Invalid SHA' >&2; exit 1;; esac
LOCK="$ROOT/.deployment-lock"
mkdir "$LOCK" 2>/dev/null || { echo '另一个部署正在执行。' >&2; exit 1; }
ROLLBACK_NEEDED=0
finish() {
  code=$?
  trap - EXIT
  if [ "$code" != 0 ] && [ "$ROLLBACK_NEEDED" = 1 ]; then
    rollback
  fi
  rmdir "$LOCK"
  exit "$code"
}
trap finish EXIT
RELEASE="$ROOT/releases/$SHA"
COMPOSE="$RELEASE/deploy/compose.yml"
CURRENT="$ROOT/current"
OLD_RELEASE=$(readlink "$CURRENT" 2>/dev/null || true)
OLD_ADMIN_ADDR=$($DOCKER inspect openlucky --format '{{range .Config.Env}}{{println .}}{{end}}' 2>/dev/null | sed -n 's/^OPENLUCKY_ADMIN_ADDR=//p' || true)
OLD_IMAGE=$($DOCKER inspect openlucky --format '{{.Config.Image}}' 2>/dev/null || true)
OLD_SHA=$($DOCKER image inspect "$OLD_IMAGE" --format '{{index .Config.Labels "org.opencontainers.image.revision"}}' 2>/dev/null || true)
REVISION=$($DOCKER image inspect "$IMAGE" --format '{{index .Config.Labels "org.opencontainers.image.revision"}}')
SOURCE=$($DOCKER image inspect "$IMAGE" --format '{{index .Config.Labels "org.opencontainers.image.source"}}')
[ "$REVISION" = "$SHA" ] && [ "$SOURCE" = 'https://github.com/yunhungo/x-open-lucky' ] || { echo '镜像标识不匹配。' >&2; exit 1; }
mkdir -p "$ROOT/data" "$ROOT/snapshots"
# 只初始化本项目数据目录权限。Docker socket 不挂载进应用。
$DOCKER run --rm --network none --user 0 --entrypoint /bin/sh -v "$ROOT/data:/data" "$IMAGE" -c 'chown 10001:10001 /data; chmod 700 /data'
export OPENLUCKY_IMAGE="$IMAGE" OPENLUCKY_ROOT="$ROOT"
if [ -f "$ROOT/runtime.env" ]; then
  # runtime.env 是 NAS 本地管理员维护的环境配置，不由更新覆盖。
  set -a
  . "$ROOT/runtime.env"
  set +a
fi
export OPENLUCKY_IMAGE="$IMAGE" OPENLUCKY_ROOT="$ROOT"
HOST=${OPENLUCKY_ADMIN_ADDR:-${OLD_ADMIN_ADDR:-127.0.0.1:24050}}
# 保留运行配置或现有容器地址；首次 NAS 部署使用规划后的管理端口。
export OPENLUCKY_ADMIN_ADDR="$HOST"
compose() { $DOCKER compose -p openlucky -f "$COMPOSE" "$@"; }
rollback() {
  trap - EXIT
  echo '新版本健康检查失败，执行回滚。' >&2
  $DOCKER logs --tail 100 openlucky > "$ROOT/deploy-failure-$SHA.log" 2>&1 || true
  chmod 600 "$ROOT/deploy-failure-$SHA.log" 2>/dev/null || true
  compose down --remove-orphans >/dev/null 2>&1 || true
  if [ -n "$OLD_IMAGE" ] && [ -n "$OLD_RELEASE" ]; then
    if [ -n "${SNAPSHOT:-}" ] && [ -f "$SNAPSHOT" ]; then
      mv "$ROOT/data" "$ROOT/failed-data-$SHA"
      $DOCKER run --rm -i --network none --user 0 --entrypoint /bin/tar -v "$ROOT:/restore" "$IMAGE" -xzf - -C /restore < "$SNAPSHOT"
    fi
    OPENLUCKY_IMAGE="$OLD_IMAGE" $DOCKER compose -p openlucky -f "$OLD_RELEASE/deploy/compose.yml" up -d --no-deps app
    RESTORED=0
    for attempt in $(seq 1 20); do
      PREVIOUS=$($DOCKER exec openlucky curl --fail --silent --max-time 5 "http://$HOST/healthz" 2>/dev/null || true)
      case "$PREVIOUS" in *"\"build_sha\":\"$OLD_SHA\""*) RESTORED=1; break;; esac
      sleep 2
    done
    [ "$RESTORED" = 1 ] || { echo '回滚容器已启动，但旧版本健康检查未通过。' >&2; rmdir "$LOCK"; exit 1; }
    echo "已恢复并验证 $OLD_IMAGE" >&2
  else
    echo '首次部署失败；配置数据已保留。' >&2
  fi
  rmdir "$LOCK"
  exit 1
}
if [ -n "$OLD_IMAGE" ]; then
  [ -n "$OLD_RELEASE" ] || { echo '发现不受此脚本管理的同名容器，已停止部署。' >&2; exit 1; }
  ROLLBACK_NEEDED=1
  $DOCKER stop --time 40 openlucky >/dev/null
  SNAPSHOT="$ROOT/snapshots/$SHA.tar.gz"
  $DOCKER run --rm --network none --user 0 --entrypoint /bin/tar -v "$ROOT/data:/data:ro" "$IMAGE" -czf - -C / data > "$SNAPSHOT.tmp"
  mv "$SNAPSHOT.tmp" "$SNAPSHOT"
  chmod 600 "$SNAPSHOT"
fi
ROLLBACK_NEEDED=1
if ! compose up -d --no-deps app; then rollback; fi
READY=0
for attempt in $(seq 1 40); do
  RESPONSE=$($DOCKER exec openlucky curl --fail --silent --max-time 5 "http://$HOST/healthz" 2>/dev/null || true)
  case "$RESPONSE" in *"\"build_sha\":\"$SHA\""*) READY=1; break;; esac
  sleep 2
done
[ "$READY" = 1 ] || rollback
ln -sfn "$RELEASE" "$CURRENT"
ROLLBACK_NEEDED=0
echo "运行版本：$RESPONSE"
# 仅清理属于 OpenLucky 的旧标记与悬空镜像；不强制、不全局 prune。
$DOCKER image ls --filter reference='openlucky:*' --format '{{.Repository}}:{{.Tag}}' | while read -r tag; do
  [ "$tag" = "$IMAGE" ] && continue
  OWNER=$($DOCKER image inspect "$tag" --format '{{index .Config.Labels "org.opencontainers.image.source"}}' 2>/dev/null || true)
  [ "$OWNER" = 'https://github.com/yunhungo/x-open-lucky' ] || continue
  $DOCKER image rm "$tag" || echo "保留被引用的镜像：$tag"
done
$DOCKER image ls --filter dangling=true --filter label=org.opencontainers.image.source=https://github.com/yunhungo/x-open-lucky --format '{{.ID}}' | while read -r id; do
  [ -n "$id" ] && { $DOCKER image rm "$id" || true; }
done
echo "入口：http://$HOST"
