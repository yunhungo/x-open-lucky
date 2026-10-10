#!/usr/bin/env bash
# @created 2026-10-11
# @description 验证源码、构建 amd64 镜像并事务部署到 QNAP。
# @author yunhungo
set -Eeuo pipefail
cd "$(dirname "$0")/.."
SHA=$(git rev-parse HEAD)
[[ -z $(git status --porcelain) ]] || { echo '请先提交所有源码改动。' >&2; exit 1; }
SSH_HOST=${OPENLUCKY_SSH_HOST:-nas}
REMOTE_ROOT=${OPENLUCKY_REMOTE_ROOT:-/share/Container/openlucky}
DOCKER_BIN=${OPENLUCKY_REMOTE_DOCKER:-/share/CACHEDEV1_DATA/.qpkg/container-station/bin/docker}
IMAGE="openlucky:sha-${SHA:0:12}"
for value in "$SSH_HOST" "$IMAGE"; do [[ "$value" =~ ^[a-zA-Z0-9@._:-]+$ ]] || exit 1; done
for value in "$REMOTE_ROOT" "$DOCKER_BIN"; do [[ "$value" =~ ^/[a-zA-Z0-9_./-]+$ && "$value" != *..* ]] || exit 1; done
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT
if [[ ${OPENLUCKY_SKIP_CHECKS:-0} != 1 ]]; then
  cargo fmt --check
  cargo clippy --all-targets --locked -- -D warnings
  cargo test --locked
  cargo build --locked
  python3 backend/tests/smoke.py
  python3 backend/tests/deployment.py
  (cd frontend && npm ci && npm run build && npx playwright test)
fi
if command -v docker-buildx >/dev/null; then BUILDX=(docker-buildx); else BUILDX=(docker buildx); fi
# Docker Desktop 卸载后可能留有失效的 credsStore。只为公开镜像拉取使用临时配置。
HELPER=$(python3 -c 'import json,os; p=os.path.expanduser("~/.docker/config.json"); print(json.load(open(p)).get("credsStore", "") if os.path.isfile(p) else "")')
if [[ -n "$HELPER" ]] && ! command -v "docker-credential-$HELPER" >/dev/null; then
  export DOCKER_HOST=${DOCKER_HOST:-$(docker context inspect --format '{{.Endpoints.docker.Host}}')}
  mkdir "$TMP_DIR/docker"
  printf '{}\n' > "$TMP_DIR/docker/config.json"
  export DOCKER_CONFIG="$TMP_DIR/docker"
fi
"${BUILDX[@]}" build --platform linux/amd64 --provenance=false --sbom=false --load --build-arg "BUILD_SHA=$SHA" --tag "$IMAGE" .
SSH=(ssh -o BatchMode=yes -o ConnectTimeout=10 "$SSH_HOST")
"${SSH[@]}" "umask 077; mkdir -p '$REMOTE_ROOT/releases/$SHA'; '$DOCKER_BIN' ps --format '{{.Names}}' >/dev/null"
echo '上传 OpenLucky 镜像…'
docker save --platform linux/amd64 "$IMAGE" | gzip -1 | "${SSH[@]}" "gzip -dc | '$DOCKER_BIN' load"
tar -czf "$TMP_DIR/release.tar.gz" deploy/compose.yml .myself/activate-qnap.sh
"${SSH[@]}" "tar -xzf - -C '$REMOTE_ROOT/releases/$SHA'" < "$TMP_DIR/release.tar.gz"
echo '发布时 OpenLucky 会有短暂中断。'
"${SSH[@]}" "sh '$REMOTE_ROOT/releases/$SHA/.myself/activate-qnap.sh' '$REMOTE_ROOT' '$SHA' '$IMAGE' '$DOCKER_BIN'"
python3 .myself/initialize-qnap.py "$SSH_HOST" "$DOCKER_BIN"
echo "部署完成：$SHA"
