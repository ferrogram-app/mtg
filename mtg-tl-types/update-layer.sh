#!/usr/bin/env bash
# Moves tl/api.tl to a newer layer and shows what changed.
#   ./update-layer.sh [FILE_OR_URL]   (default: tdesktop's scheme; a URL needs curl)
#   FORCE=1 ./update-layer.sh FILE    (accept the same or an older layer)
# Then follow UPGRADING.md.
set -eu
export LC_ALL=C
cd "$(dirname "$0")"
src=${1:-https://raw.githubusercontent.com/telegramdesktop/tdesktop/dev/Telegram/SourceFiles/mtproto/scheme/api.tl}
new=$(mktemp)
trap 'rm -f "$new" "$new.old" "$new.new"' EXIT
case $src in
  http*) curl -fsSL "$src" -o "$new" ;;
  *) cp "$src" "$new" ;;
esac

layer_of() { sed -n '1,5s|^// LAYER \([0-9][0-9]*\).*|\1|p' "$1" | head -n 1; }
old=$(layer_of tl/api.tl)
now=$(layer_of "$new")
[ -n "$now" ] || { echo "no '// LAYER N' line in the new file" >&2; exit 1; }
echo "layer $old -> $now"
if [ "$now" -le "$old" ] && [ -z "${FORCE:-}" ]; then
  echo "not newer (FORCE=1 to accept)" >&2
  exit 1
fi

# one line per definition: "name<TAB>whole line"
defs() { awk '/^[A-Za-z0-9_.]+#[0-9a-f]+ /{k=$1; sub(/#.*/,"",k); print k "\t" $0}' "$1" | sort; }
defs tl/api.tl | cut -f1 | sort -u > "$new.old"
defs "$new" | cut -f1 | sort -u > "$new.new"
echo "--- removed:"; comm -23 "$new.old" "$new.new"
echo "--- added:";   comm -13 "$new.old" "$new.new"
echo "--- changed (same name, other id or fields):"
join -t "$(printf '\t')" <(defs tl/api.tl) <(defs "$new") | awk -F'\t' '$2 != $3 { print $1 }'

cp "$new" tl/api.tl
echo "tl/api.tl updated; building..."
cargo build -p mtg-tl-types 2>&1 | tail -n 5
