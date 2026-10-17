#!/bin/sh
# Runs install.sh then uninstall.sh of a Linux package inside a throwaway home, and checks
# the .shoto file type they declare and take back. Nothing is written outside that home.
# Usage: sh test-linux-install.sh <package folder> <throwaway folder>
set -eu
package=${1:?package folder}
throwaway=${2:?throwaway folder}
# A home whose name needs quoting in a desktop entry: space, quotes, dollar, backslash. Not %:
# install.sh doubles it as the rules ask, but GLib checks the program path without undoing it.
home="$throwaway/odd \"home\" \$x back\\slash"
rm -rf "$throwaway"
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

# The program and arguments a desktop entry launches, one per line: its Exec= line undone
# by the Desktop Entry rules (string escapes, then quoted arguments, then %%).
launched() {
    sed -n 's/^Exec=//p' "$1" | awk '{
        s = ""
        for (i = 1; i <= length($0); i++) {
            c = substr($0, i, 1)
            if (c == "\\" && substr($0, i + 1, 1) == "\\") i++
            s = s c
        }
        arg = ""; quoted = 0
        for (i = 1; i <= length(s); i++) {
            c = substr(s, i, 1)
            if (quoted && c == "\\") { i++; arg = arg substr(s, i, 1) }
            else if (c == "\"") quoted = !quoted
            else if (!quoted && c == " ") { if (arg != "") print arg; arg = "" }
            else arg = arg c
        }
        if (arg != "") print arg
    }' | sed 's/%%/%/g'
}
expect_launch() {
    [ "$(launched "$1")" = "$2" ] || fail "$1 launches $(launched "$1" | tr '\n' ' ')"
}

sh "$package/install.sh" >/dev/null
[ -x "$home/.local/bin/shotoshot" ] || fail "the program is not installed"
[ -f "$data/mime/packages/shotoshot.xml" ] || fail "the file type is not declared"
grep -q '^MimeType=image/x-shotoshot;$' "$entry" || fail "no entry opens the file type"
program="$home/.local/bin/shotoshot"
expect_launch "$entry" "$program
%f"
expect_launch "$data/applications/shotoshot.desktop" "$program
--capture"
expect_launch "$home/.config/autostart/shotoshot.desktop" "$program"
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
rm -rf "$throwaway"
echo "install: entries launch from an odd home, .shoto type declared, then removed ($queried)"
