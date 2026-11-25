#!/usr/bin/env bash
# Runs EQaura's generated settings inside a private PipeWire with a virtual
# output, plays a 1 kHz tone through it and compares the recording with the
# drawn curve. Needs no sound hardware and makes no audible sound.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -z "${EQAURA_TEST_BUS:-}" ]; then
  EQAURA_TEST_BUS=1 exec dbus-run-session -- bash "$0"
fi
work=$(mktemp -d)
export XDG_RUNTIME_DIR="$work/run" XDG_CONFIG_HOME="$work/config" XDG_STATE_HOME="$work/state"
mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
trap 'kill $(jobs -p) 2>/dev/null || true' EXIT
cargo build -q -p eq-backend --example pipewire_text
tool=target/debug/examples/pipewire_text

pipewire >"$work/pipewire.log" 2>&1 &
sleep 1
wireplumber >"$work/wireplumber.log" 2>&1 &
sleep 2
pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=test-sink node.description=Test media.class=Audio/Sink object.linger=true audio.position=[FL FR] audio.rate=48000 monitor.channel-volumes=true }' >/dev/null
sleep 1
$tool config >"$work/eqaura.conf"
pipewire -c "$work/eqaura.conf" >"$work/engine.log" 2>&1 &
sleep 2
pipewire --version | sed -n 2p
wpctl status | sed -n '/Audio/,/Video/p'

python3 - "$work/tone.wav" <<'PY'
import math, struct, sys, wave
w = wave.open(sys.argv[1], "wb")
w.setnchannels(2); w.setsampwidth(2); w.setframerate(48000)
w.writeframes(b"".join(struct.pack("<hh", v, v) for v in (int(8000 * math.sin(2 * math.pi * 1000 * i / 48000)) for i in range(48000 * 5))))
PY

measure() {
  pw-play --target eqaura.sink "$work/tone.wav" &
  player=$!
  sleep 1.5
  timeout 2 pw-record -P '{ stream.capture.sink = true }' --target test-sink --rate 48000 --channels 2 --format s16 "$work/rec.wav" || true
  wait $player || true
  python3 - "$work/rec.wav" <<'PY'
import math, struct, sys
data = open(sys.argv[1], "rb").read()[44:]
n = len(data) // 4
frames = struct.unpack("<%dh" % (2 * n), data[:4 * n])
part = range(n // 4, n * 3 // 4)
for ch in (0, 1):
    rms = math.sqrt(sum(frames[2 * i + ch] ** 2 for i in part) / len(part))
    print("%.3f" % (20 * math.log10(max(rms, 1e-9) / (8000 / math.sqrt(2)))), end=" ")
print()
PY
}

measure | tee "$work/measured"
id=$(pw-dump | python3 -c 'import json,sys; print(next(o["id"] for o in json.load(sys.stdin) if (o.get("info") or {}).get("props",{}).get("node.name")=="eqaura.sink"))')
$tool live | while IFS= read -r params; do pw-cli set-param "$id" Props "$params" >/dev/null </dev/null; done
measure | tee -a "$work/measured"
$tool expect >"$work/expected"
echo "expected:"; cat "$work/expected"
python3 - "$work/measured" "$work/expected" <<'PY' || { tail -20 "$work"/*.log; exit 1; }
import sys
a, b = ([list(map(float, l.split())) for l in open(p)] for p in sys.argv[1:3])
worst = max(abs(x - y) for r, s in zip(a, b) for x, y in zip(r, s))
print("largest difference: %.3f dB" % worst)
sys.exit(worst > 0.1 or len(a) != 2)
PY
