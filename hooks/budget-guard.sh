#!/bin/bash
# UserPromptSubmit: corta el turno cuando el gasto ya paso un techo.
#
# Que techos hace cumplir, y si esta prendido, se configura en claude-burn ->
# Ajustes -> Bloqueo. Aca no hay nada que editar: eso es a proposito, porque
# el bloqueo tambien se come el mensaje con el que pedirias desbloquearlo.
#
# No es un tope a prueba de balas: corta el proximo prompt una vez que detecta
# que ya te pasaste, no puede devolver lo que ya se gasto.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
st=$("$SCRIPT_DIR/burn.sh")

[ "$(echo "$st" | jq -r '.guard_enabled // false')" = "true" ] || exit 0

# El primer techo pasado gana; se informa ese y no todos.
read -r label spent limit <<<"$(echo "$st" | jq -r '
  . as $s
  | (.guard_periods // ["daily"])
  | map(
      if   . == "daily"   then {l: "diario",  s: ($s.today // 0), b: ($s.day_budget   // 0)}
      elif . == "weekly"  then {l: "semanal", s: ($s.week  // 0), b: ($s.week_budget  // 0)}
      elif . == "monthly" then {l: "mensual", s: ($s.month // 0), b: ($s.month_budget // 0)}
      else empty end
    )
  | map(select(.b > 0 and .s > .b))
  | if length == 0 then "" else (.[0] | [.l, .s, .b] | @tsv) end')"

[ -n "$label" ] || exit 0

jq -n --arg label "$label" \
      --arg cost "$(awk -v c="$spent" 'BEGIN{printf "%.2f", c}')" \
      --arg limit "$(awk -v b="$limit" 'BEGIN{printf "%.0f", b}')" \
  '{decision: "block", reason: ("🚫 Techo " + $label + " de $" + $limit + " alcanzado (llevás $" + $cost + "). Para seguir igual, apagá o subí el bloqueo en claude-burn → Ajustes → Bloqueo — desde la app, no desde este chat, porque este mismo mensaje también quedaría bloqueado.")}'
