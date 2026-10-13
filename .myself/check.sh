#!/usr/bin/env bash
# @created 2026-10-11
# @description 执行后台质量检查、协议集成测试和真实浏览器验证。
# @author yunhungo
set -Eeuo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --locked --examples
cargo build --locked
(cd frontend && npm ci && npm run build)
python3 backend/tests/smoke.py
python3 backend/tests/web_protocols.py
python3 backend/tests/shutdown.py
python3 backend/tests/deployment.py
(cd frontend && npx playwright test)
bash -n .myself/build-and-deploy-qnap.sh
sh -n .myself/activate-qnap.sh
