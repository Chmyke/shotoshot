#!/bin/sh
# Runs install.sh then uninstall.sh of a Linux package inside a throwaway home, and checks
# the .shoto file type they declare and take back. Nothing is written outside that home.
# Usage: sh test-linux-install.sh <package folder> <throwaway home>
set -eu
package=${1:?package folder}
home=${2:?throwaway home}
rm -rf "$home"
mkdir -p "$home"
export HOME="$home"
unset XDG_DATA_HOME XDG_CONFIG_HOME
data="$home/.local/share"
entry="$data/applications/shotoshot-open.desktop"
fail() {
    echo "FAILED: install: $1"
    exit 1
}
# What the desktop takes a .shoto file for; its content is a PNG, as in a real one.
seen() {
    gio info -a standard::content-type "$sample" | sed -n 's/^ *standard::content-type: //p'
}

sh "$package/install.sh" >/dev/null
[ -x "$home/.local/bin/shotoshot" ] || fail "the program is not installed"
[ -f "$data/mime/packages/shotoshot.xml" ] || fail "the file type is not declared"
grep -q '^MimeType=image/x-shotoshot;$' "$entry" || fail "no entry opens the file type"
grep -q "^Exec=$home/.local/bin/shotoshot %f\$" "$entry" || fail "the entry does not pass the file"
sample="$home/Shot 001.shoto"
cp "$package/icons/shotoshot-32.png" "$sample"
if command -v gio >/dev/null 2>&1; then
    [ "$(seen)" = image/x-shotoshot ] || fail "a .shoto file is taken for $(seen)"
    gio mime image/x-shotoshot | head -n 1 | grep -q shotoshot-open.desktop ||
        fail "a .shoto file does not open in Shotoshot"
    queried="type and program asked to the desktop"
else
    queried="files only: no gio here"
fi

sh "$package/uninstall.sh" >/dev/null
[ ! -e "$home/.local/bin/shotoshot" ] || fail "the program outlives the removal"
[ ! -e "$entry" ] && [ ! -e "$data/mime/packages/shotoshot.xml" ] ||
    fail "the file type outlives the removal"
if command -v gio >/dev/null 2>&1; then
    [ "$(seen)" != image/x-shotoshot ] || fail "the desktop still knows the removed file type"
fi
rm -rf "$home"
echo "install: .shoto type declared, then removed ($queried)"
