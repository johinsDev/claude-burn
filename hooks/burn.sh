#!/bin/bash
# Lee los techos y el gasto de claude-burn, que es la unica fuente de verdad:
# los limites se editan en la app (Alertas -> Presupuesto), no aca.
#
# Reemplaza a window-cost.sh, que llamaba a `npx ccusage` en cada prompt. Eso
# arrancaba un node por turno y tardaba lo suficiente como para necesitar 15
# minutos de cache; esto tarda ~17ms y ademas cuenta los subagentes, que
# ccusage no ve porque viven en transcripts aparte.
set -euo pipefail

BURN_CLI="${BURN_CLI:-$HOME/.local/bin/burn-cli}"

# Sin la herramienta instalada los hooks no deben romper la sesion.
[ -x "$BURN_CLI" ] || { echo '{}'; exit 0; }
"$BURN_CLI" status 2>/dev/null || echo '{}'
