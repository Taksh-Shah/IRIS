#!/usr/bin/env bash
# Tier-2 Wi-Fi Direct cold-start gate — the ACTUAL hardware verification.
#
# Loop §2 gate: "cold-start both phones 10x -> exactly one group forms ->
# messages flow both ways every time" + the operator's hard "no OS invitation
# dialog" pass/fail criterion (Session 24).
#
# Why this and not `iris_bench` (Mobly): vivo/OEM Android gates
# WifiP2pManager.discoverPeers() behind "the calling app has a foreground
# Activity". The Mobly instrumented snippet process has none, so its engine's
# discovery finds peers_seen=0 forever. This script drives the REAL shipping
# app (which has MainActivity) over adb instead — the only path that actually
# exercises the Tier-2 code on this hardware today. `test_tier2_wifidirect.py`
# in iris_bench carries the same logic and stays HW-PENDING on the discovery
# gate (see its docstring).
#
# Prereqs, run once by hand before this script:
#   - both phones' IRIS app built & installed, Bluetooth OFF on both
#     (so Wi-Fi Direct is provably the transport under test)
#   - /addkey run BOTH ways once (keys persist across restarts since
#     Session 26 — X25519StaticAd + KnownPeersStore), OR pass the node/x25519
#     env vars below and let cycle 0 not assert delivery
#
# Usage:  P1=<serial> P2=<serial> P1N=<64hex> P2N=<64hex> \
#         bash tier2_wifidirect_cold_cycle.sh [cycles]
set -u
P1=${P1:-10BCA20F4M000BB}
P2=${P2:-b2fbcd39}
P1N=${P1N:?set P1N to P1's 64-hex node id}
P2N=${P2N:?set P2N to P2's 64-hex node id}
CYCLES=${1:-10}
PKG=org.iris.mesh

# tap the console send-arrow (right of the EditText, vertical centre)
send() {  # send <serial> <text-with-%s-for-space>
  local s=$1 c=$2 b x1 y1 x2 y2
  b=$(adb -s "$s" exec-out uiautomator dump /dev/tty 2>/dev/null | tr '>' '\n' \
      | grep EditText | grep -oE 'bounds="\[[0-9]+,[0-9]+\]\[[0-9]+,[0-9]+\]"' \
      | head -1 | grep -oE '[0-9]+')
  read -r x1 y1 x2 y2 <<<"$b"
  adb -s "$s" shell input tap $(((x1+x2)/2)) $(((y1+y2)/2)); sleep 0.4
  adb -s "$s" shell input keyevent KEYCODE_MOVE_END
  adb -s "$s" shell "input keyevent $(yes 67 | head -90 | tr '\n' ' ')"; sleep 0.2
  MSYS_NO_PATHCONV=1 adb -s "$s" shell input text "$c"; sleep 0.4
  b=$(adb -s "$s" exec-out uiautomator dump /dev/tty 2>/dev/null | tr '>' '\n' \
      | grep EditText | grep -oE 'bounds="\[[0-9]+,[0-9]+\]\[[0-9]+,[0-9]+\]"' \
      | head -1 | grep -oE '[0-9]+')
  read -r x1 y1 x2 y2 <<<"$b"
  adb -s "$s" shell input tap $((x2+95)) $(((y1+y2)/2)); sleep 0.3
  adb -s "$s" shell input tap $((x2+95)) $(((y1+y2)/2)); sleep 0.6
}

pass=0
for i in $(seq 1 "$CYCLES"); do
  echo "===== CYCLE $i/$CYCLES ====="
  for d in "$P1" "$P2"; do adb -s "$d" shell am force-stop $PKG; done
  sleep 2
  for d in "$P1" "$P2"; do
    adb -s "$d" logcat -c
    adb -s "$d" shell monkey -p $PKG -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1
  done
  t=0
  while :; do
    g1=$(adb -s "$P1" shell dumpsys wifip2p 2>/dev/null | grep -c "groupFormed: true")
    g2=$(adb -s "$P2" shell dumpsys wifip2p 2>/dev/null | grep -c "groupFormed: true")
    [ "$g1" = 1 ] && [ "$g2" = 1 ] && break
    t=$((t+5)); [ $t -ge 120 ] && { echo "CYCLE $i: GROUP TIMEOUT"; break; }
    sleep 5
  done
  [ "$g1" = 1 ] && [ "$g2" = 1 ] || { echo "CYCLE $i FAIL: no group"; continue; }
  echo "CYCLE $i: group formed in ${t}s"
  sleep 3
  adb -s "$P1" logcat -c; adb -s "$P2" logcat -c
  send "$P1" "/to%s${P2N}" >/dev/null; send "$P1" "C${i}-fwd" >/dev/null
  sleep 5
  fwd=$(adb -s "$P2" logcat -d 2>&1 | grep -c "msg.delivered")
  adb -s "$P1" logcat -c; adb -s "$P2" logcat -c
  send "$P2" "/to%s${P1N}" >/dev/null; send "$P2" "C${i}-rev" >/dev/null
  sleep 5
  rev=$(adb -s "$P1" logcat -d 2>&1 | grep -c "msg.delivered")
  dlg=$(for d in "$P1" "$P2"; do adb -s "$d" logcat -d 2>&1; done | grep -ic "invitation to connect")
  echo "CYCLE $i: fwd=$fwd rev=$rev dialog=$dlg"
  [ "$fwd" -ge 1 ] && [ "$rev" -ge 1 ] && [ "$dlg" = 0 ] && pass=$((pass+1))
done
echo "================ GATE: $pass/$CYCLES ================"
[ "$pass" = "$CYCLES" ]
