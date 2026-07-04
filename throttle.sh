#!/usr/bin/env bash
# Cap this Mac's INBOUND web bandwidth with the built-in dummynet shaper
# (pf + dnctl). Works on already-running downloads. Requires root.
#
#   sudo ./throttle.sh on 5     # cap inbound 80/443 to 5 MB/s (default 5)
#   sudo ./throttle.sh off      # remove cap, restore default pf ruleset
#   sudo ./throttle.sh status
#
# Shapes ALL inbound HTTP/HTTPS on the machine — HuggingFace/Xet use many CDN
# IPs, so per-host shaping isn't practical. Turn it off when downloads finish.
set -euo pipefail

ANCHOR="dlthrottle"
cmd="${1:-status}"
mbps="${2:-5}"                                   # MB/s

if [ "$cmd" != "status" ] && [ "$(id -u)" -ne 0 ]; then
  echo "needs root:  sudo $0 $*" >&2; exit 1
fi

case "$cmd" in
  on)
    mbit=$(awk "BEGIN{printf \"%d\", $mbps*8}")   # MB/s -> Mbit/s
    dnctl pipe 1 config bw "${mbit}Mbit/s"
    # append our anchor to the live pf ruleset, then load the shaping rule
    (cat /etc/pf.conf; echo "dummynet-anchor \"$ANCHOR\""; echo "anchor \"$ANCHOR\"") | pfctl -f -
    # shape ALL inbound (any proto/port) so nothing slips past the cap; the
    # narrower "port { 80, 443 }" rule leaked ~30% (stray ports / non-TCP).
    echo 'dummynet in quick from any to any pipe 1' | pfctl -a "$ANCHOR" -f -
    pfctl -E
    echo "Throttle ON: inbound 80/443 -> ${mbps} MB/s (${mbit} Mbit/s)."
    ;;
  off)
    pfctl -a "$ANCHOR" -F all 2>/dev/null || true
    dnctl -q flush 2>/dev/null || true
    pfctl -f /etc/pf.conf 2>/dev/null || true
    echo "Throttle OFF: pipe flushed, default pf ruleset restored."
    echo "(pf is left enabled with default rules; 'sudo pfctl -d' disables pf entirely.)"
    ;;
  status)
    dnctl list 2>/dev/null || echo "dnctl needs sudo to query"
    ;;
  *) echo "usage: sudo $0 on|off|status [MB/s]" ; exit 1 ;;
esac
