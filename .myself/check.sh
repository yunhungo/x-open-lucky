#!/usr/bin/env bash
# @created 2026-10-11
# @description 统一执行完整验证或指定阶段，供本机与 CI 复用。
# @author yunhungo
set -Eeuo pipefail
cd "$(dirname "$0")/.."
if [[ $# == 0 ]]; then
  set -- rust frontend smoke groups protocols shutdown deployment browser shell
fi
# 在运行任何检查前拒绝拼写错误，避免只验证了部分内容却误报成功。
for stage in "$@"; do
  case "$stage" in
    rust|frontend|smoke|groups|protocols|shutdown|deployment|browser|shell) ;;
    *) printf 'Unknown verification stage: %s\n' "$stage" >&2; exit 2 ;;
  esac
done
trap 'code=$?; printf "Verification failed: %s (exit %s)\n" "$stage" "$code" >&2; exit "$code"' ERR
for stage in "$@"; do
  printf 'Checking %s\n' "$stage"
  case "$stage" in
    rust)
      cargo fmt --check
      cargo clippy --all-targets --locked -- -D warnings
      cargo test --locked
      cargo build --locked --examples
      cargo build --locked
      ;;
    frontend) (cd frontend && npm ci && npm run build) ;;
    smoke) python3 backend/tests/smoke.py ;;
    groups) python3 backend/tests/groups.py ;;
    protocols) python3 backend/tests/web_protocols.py ;;
    shutdown) python3 backend/tests/shutdown.py ;;
    deployment) python3 backend/tests/deployment.py ;;
    browser) (cd frontend && npx playwright test) ;;
    shell)
      bash -n .myself/check.sh .myself/check-image.sh .myself/build-and-deploy-qnap.sh
      sh -n .myself/activate-qnap.sh
      ;;
  esac
done
