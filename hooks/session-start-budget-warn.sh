#!/bin/bash
# SessionStart: aviso pasivo, nunca bloquea. Solo habla cuando hay algo que
# decir — arriba del 80% del dia, o el mes pasado de su techo.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
st=$("$SCRIPT_DIR/burn.sh")
read -r today budget month month_budget <<<"$(echo "$st" | jq -r '
  [(.today // 0), (.day_budget // 0), (.month // 0), (.month_budget // 0)] | @tsv')"

msg=""
if awk -v b="$budget" 'BEGIN{exit !(b>0)}'; then
  pct=$(awk -v c="$today" -v b="$budget" 'BEGIN{printf "%.0f", (c/b)*100}')
  if [ "$pct" -ge 100 ]; then
    msg=$(awk -v c="$today" -v b="$budget" \
      'BEGIN{printf "🚨 Ya llevás $%.2f hoy — pasaste el techo diario de $%.0f.", c, b}')
  elif [ "$pct" -ge 80 ]; then
    msg=$(awk -v c="$today" -v b="$budget" -v p="$pct" \
      'BEGIN{printf "⚠️ Llevás $%.2f de $%.0f hoy (%s%%).", c, b, p}')
  fi
fi

if awk -v m="$month" -v b="$month_budget" 'BEGIN{exit !(b>0 && m>b)}'; then
  extra=$(awk -v m="$month" -v b="$month_budget" \
    'BEGIN{printf "🔴 El mes va en $%.0f contra un techo de $%.0f.", m, b}')
  msg="${msg:+$msg }$extra"
fi

[ -n "$msg" ] || exit 0
jq -n --arg msg "$msg" '{systemMessage: $msg}'
