#!/usr/bin/env sh
# Downloads the QLC+ fixture definitions and input profiles (Apache License 2.0) into
# fixtures/qlcplus and profiles/qlcplus. They are not stored in this repository.
#
#   scripts/update-qlcplus-fixtures.sh              download / refresh
#   scripts/update-qlcplus-fixtures.sh --if-missing only if not present yet (builds, CI)
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
dest="$root/fixtures/qlcplus"
profiles_dest="$root/profiles/qlcplus"

if [ "${1:-}" = "--if-missing" ] && [ -f "$dest/LICENSE" ] && [ -f "$profiles_dest/LICENSE" ]; then
  echo "fixtures/qlcplus and profiles/qlcplus already present"
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git clone -q --depth 1 --filter=blob:none --sparse https://github.com/mcallegari/qlcplus "$tmp/qlcplus"
git -C "$tmp/qlcplus" sparse-checkout set resources/fixtures resources/inputprofiles
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
# Input (controller) profiles, flat: one .qxi per controller.
rm -rf "$profiles_dest"
mkdir -p "$profiles_dest"
cp "$tmp"/qlcplus/resources/inputprofiles/*.qxi "$profiles_dest/"
cp "$tmp/qlcplus/COPYING" "$profiles_dest/LICENSE"
cat > "$profiles_dest/README.md" <<EOF
# QLC+ input profiles

Controller profiles (\`.qxi\`) from the QLC+ project
(<https://github.com/mcallegari/qlcplus>, \`resources/inputprofiles\`),
commit \`$commit\`, licensed under the Apache License 2.0 (see \`LICENSE\`).

Downloaded by \`scripts/update-qlcplus-fixtures.sh\`; not stored in the kLight repository.
EOF
profile_count=$(find "$profiles_dest" -name '*.qxi' | wc -l | tr -d ' ')
echo "profiles/qlcplus: $profile_count input profiles"
count=$(find "$dest" -name '*.qxf' | wc -l | tr -d ' ')
echo "fixtures/qlcplus: $count definitions from QLC+ $commit"
