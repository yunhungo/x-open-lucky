#!/usr/bin/env bash
# @created 2026-10-14
# @description 启动待验收镜像并核对健康、构建提交、前端与 ACME 客户端。
# @author yunhungo
set -Eeuo pipefail
IMAGE=${1:?Image tag is required}
EXPECTED_SHA=${2:?Expected build SHA is required}
CONTAINER=''
finish() {
  code=$?
  if [[ -n "$CONTAINER" ]]; then
    if [[ $code != 0 ]]; then docker logs --tail 100 "$CONTAINER" >&2 || true; fi
    docker rm -f "$CONTAINER" >/dev/null || true
  fi
  exit "$code"
}
trap finish EXIT
docker run --rm --platform linux/amd64 --entrypoint lego "$IMAGE" --version
# 不挂载宿主数据，也不发布端口；只删除本脚本创建的临时容器。
CONTAINER=$(docker run -d --platform linux/amd64 "$IMAGE")
HEALTH=$(docker exec "$CONTAINER" curl --retry 10 --retry-connrefused --retry-delay 1 --max-time 5 --fail --silent --show-error http://127.0.0.1:56002/healthz)
EXPECTED_SHA="$EXPECTED_SHA" HEALTH="$HEALTH" python3 - <<'PY'
import json
import os

health = json.loads(os.environ['HEALTH'])
assert health['ok'] is True
assert health['build_sha'] == os.environ['EXPECTED_SHA'], 'Image build SHA mismatch'
print('PASS image health and build SHA')
PY
docker exec "$CONTAINER" curl --max-time 5 --fail --silent --output /dev/null 'http://127.0.0.1:56002/web?group=unassigned'
echo 'PASS image frontend route and bundled ACME'
