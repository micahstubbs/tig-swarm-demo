#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
(cd dashboard && yarn install --frozen-lockfile && yarn build)
rsync -a --delete dashboard/dist/ server/static/
echo "Staged $(find server/static -type f | wc -l) files into server/static/"
