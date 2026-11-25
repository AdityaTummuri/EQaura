#!/usr/bin/env bash
# Runs on a booted emulator. Usage: android_test.sh <folder with apks> <output folder>
set -euo pipefail
apks="$1" out="$2"
app=io.github.adityatummuri.eqaura
shots=/sdcard/Android/media/$app/shots
mkdir -p "$out"
find_apk() { find "$apks" -name "$1" | head -1; }

finish() {
  adb logcat -d > "$out/logcat.txt" || true
  grep -E 'EQaura *: measured|: measured' "$out/logcat.txt" | sed 's/.*EQaura *: //' || true
}
trap finish EXIT
# Dialogs of other slow apps must not cover ours.
adb shell settings put global hide_error_dialogs 1
adb logcat -G 8M
adb logcat -c

echo "== Tests"
adb install -r -t "$(find_apk 'app-x86_64-debug.apk')"
adb install -r -t "$(find_apk '*androidTest*.apk')"
adb shell mkdir -p "$shots"
adb shell am instrument -w -e shots "$shots" "$app.test/androidx.test.runner.AndroidJUnitRunner" | tee "$out/tests.txt"
adb pull "$shots" "$out/shots" || true
passed=0
if grep -qE '^OK \(' "$out/tests.txt"; then passed=1; fi
adb uninstall "$app.test" >/dev/null
adb uninstall "$app" >/dev/null

echo "== Released app"
adb install "$(find_apk 'EQaura-*-x86_64.apk')"
adb shell pm grant "$app" android.permission.POST_NOTIFICATIONS 2>/dev/null || true
adb shell am start -W -n "$app/.MainActivity"
sleep 6
# Presses a button by its text.
press() {
  for _ in $(seq 8); do
    if press_once "$1"; then return 0; fi
    # Slow emulators show "isn't responding" for their own launcher.
    press_once "Wait" >/dev/null 2>&1 || true
    sleep 5
  done
  return 1
}
press_once() {
  adb shell uiautomator dump /sdcard/ui.xml >/dev/null 2>&1 || return 1
  adb shell cat /sdcard/ui.xml > "$out/ui.xml"
  python3 - "$out/ui.xml" "$1" > "$out/tap" <<'PY' || return 1
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
m = re.search(r'text="%s"[^>]*?bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"' % re.escape(sys.argv[2]), text)
if not m:
    sys.exit(f"No button named {sys.argv[2]}")
a, b, c, d = map(int, m.groups())
print((a + c) // 2, (b + d) // 2)
PY
  # shellcheck disable=SC2046
  adb shell input tap $(cat "$out/tap")
}
alive() {
  adb shell pidof "$app" >/dev/null || { echo "The app is not running: $1"; exit 1; }
  if adb logcat -d -b crash | grep -q "Process: $app"; then echo "The app crashed: $1"; adb logcat -d -b crash; exit 1; fi
}
service() { adb shell dumpsys activity services "$app" | grep -q 'isForeground=true'; }
adb logcat -b crash -c
press "Turn on"
sleep 5
alive "after turning on"
service || { echo "The service did not start"; adb shell dumpsys activity services "$app"; exit 1; }
adb exec-out screencap -p > "$out/released.png"
adb shell dumpsys media.audio_flinger | grep -iE -A3 'dynamics' | head -20 || true

echo "== Screen off and memory"
adb shell input keyevent KEYCODE_HOME
adb shell input keyevent KEYCODE_SLEEP
sleep 20
service || { echo "The service ended in the background"; exit 1; }
adb shell dumpsys meminfo "$app" | grep -E 'TOTAL( PSS)?:' | head -1

echo "== Restart"
adb reboot
sleep 5
adb wait-for-device
for _ in $(seq 90); do
  [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] && break
  sleep 2
done
back=0
for _ in $(seq 45); do
  if service; then back=1; break; fi
  sleep 2
done
[ "$back" = 1 ] || { echo "The equalizer did not come back after a restart"; adb logcat -d | grep -iE 'eqaura|ActivityManager.*Background' | tail -30; exit 1; }
alive "after a restart"

if [ "$passed" != 1 ]; then echo "Some tests failed, see above"; exit 1; fi
echo "All good"
