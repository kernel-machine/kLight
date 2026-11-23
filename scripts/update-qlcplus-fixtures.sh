#!/usr/bin/env sh
# Downloads the QLC+ fixture definitions (Apache License 2.0) into
# fixtures/qlcplus. They are not stored in this repository.
#
#   scripts/update-qlcplus-fixtures.sh              download / refresh
#   scripts/update-qlcplus-fixtures.sh --if-missing only if not present yet (builds, CI)
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
dest="$root/fixtures/qlcplus"

if [ "${1:-}" = "--if-missing" ] && [ -f "$dest/LICENSE" ]; then
  echo "fixtures/qlcplus already present"
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git clone -q --depth 1 --filter=blob:none --sparse https://github.com/mcallegari/qlcplus "$tmp/qlcplus"
git -C "$tmp/qlcplus" sparse-checkout set resources/fixtures
commit=$(git -C "$tmp/qlcplus" log -1 --format=%H)

rm -rf "$dest"
mkdir -p "$dest"
cp -r "$tmp/qlcplus/resources/fixtures/." "$dest/"
# Keep only the definitions (one folder per manufacturer).
find "$dest" -type f ! -name '*.qxf' -delete
find "$dest" -type d -empty -delete 2>/dev/null || true
mkdir -p "$dest"
cp "$tmp/qlcplus/COPYING" "$dest/LICENSE"
cat > "$dest/README.md" <<EOF
# QLC+ fixture definitions

Fixture definitions (\`.qxf\`) from the QLC+ project
(<https://github.com/mcallegari/qlcplus>, \`resources/fixtures\`),
commit \`$commit\`, licensed under the Apache License 2.0 (see \`LICENSE\`).
Copyright of the individual definitions belongs to their authors (see the
\`<Creator>\` element of each file).

Downloaded by \`scripts/update-qlcplus-fixtures.sh\`; not stored in the kLight repository.
EOF
count=$(find "$dest" -name '*.qxf' | wc -l | tr -d ' ')
echo "fixtures/qlcplus: $count definitions from QLC+ $commit"
