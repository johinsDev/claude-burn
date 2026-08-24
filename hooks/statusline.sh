#!/bin/bash
# statusLine: modelo + gasto de hoy contra el techo diario, y el mes si ya se
# paso. Corto a proposito para que no lo trunque un panel angosto.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
input=$(cat)
model=$(echo "$input" | jq -r '.model.display_name // "Claude"')

st=$("$SCRIPT_DIR/burn.sh")
read -r today budget month month_budget <<<"$(echo "$st" | jq -r '
  [(.today // 0), (.day_budget // 0), (.month // 0), (.month_budget // 0)] | @tsv')"

# Sin techo configurado no hay porcentaje que mostrar, solo el gasto.
if awk -v b="$budget" 'BEGIN{exit !(b>0)}'; then
  pct=$(awk -v c="$today" -v b="$budget" 'BEGIN{printf "%.0f", (c/b)*100}')
  icon="💰"
  [ "$pct" -ge 80 ] && icon="⚠️"
  [ "$pct" -ge 100 ] && icon="🚨"
  line=$(awk -v i="$icon" -v c="$today" -v b="$budget" -v p="$pct" \
    'BEGIN{printf "%s $%.2f/$%.0f hoy (%s%%)", i, c, b, p}')
else
  line=$(awk -v c="$today" 'BEGIN{printf "💰 $%.2f hoy", c}')
fi

# El mes solo aparece cuando ya se paso: si no, es ruido en cada turno.
if awk -v m="$month" -v b="$month_budget" 'BEGIN{exit !(b>0 && m>b)}'; then
  mpct=$(awk -v m="$month" -v b="$month_budget" 'BEGIN{printf "%.0f", (m/b)*100}')
  line="$line | 🔴 mes ${mpct}%"
fi

echo "🤖 ${model} | ${line}"
