# shellcheck shell=bash
# Texts of the installer scripts, in the language chosen for the
# installation. They live in the "installer" section of the language files
# (locales/<code>.json, and /etc/xbnc/locales/<code>.json for languages
# added later), next to the texts of IRC and of the panel, so translating
# xBNC into a language also translates its installer. English is the base:
# a text missing from a language comes out in English.
#
# Sourced by install.sh, prereqs.sh, binary.sh and update.sh, which set
# SRC_DIR. Needs only bash and awk (no Python, no jq), so it works on a
# freshly installed server.
#
#   xbnc_load_texts CODE      load the texts of CODE over the English ones
#   t KEY [ARG...]            print a text, with {0}, {1}... replaced by ARG
#   xbnc_languages            list "code<TAB>name<TAB>english name" of every language file

XBNC_LOCALE_DIRS=("$SRC_DIR/locales" /etc/xbnc/locales)
declare -gA XBNC_T=()

# Prints "key<TAB>value" for every text of one top-level object of a JSON
# file. A small JSON reader: it accepts any valid formatting (one line or
# many, any order), decodes the escapes (\" \\ \n \uXXXX, surrogate pairs
# included) and ignores everything that is not a text. Line breaks and tabs
# inside a text become spaces.
xbnc_json_section() { # FILE SECTION
    LC_ALL=C awk -v want="$2" '
    function utf8(c) {
        if (c < 128) return sprintf("%c", c)
        if (c < 2048) return sprintf("%c%c", 192 + int(c / 64), 128 + c % 64)
        if (c < 65536) return sprintf("%c%c%c", 224 + int(c / 4096), 128 + int(c / 64) % 64, 128 + c % 64)
        return sprintf("%c%c%c%c", 240 + int(c / 262144), 128 + int(c / 4096) % 64, 128 + int(c / 64) % 64, 128 + c % 64)
    }
    function hex(s,   i, n, d) {
        n = 0
        for (i = 1; i <= length(s); i++) {
            d = index("0123456789abcdef", tolower(substr(s, i, 1)))
            if (d == 0) return -1
            n = n * 16 + d - 1
        }
        return n
    }
    # Reads the string that starts at buf[pos] (an opening quote); leaves
    # pos after the closing quote.
    function str(   out, c, e, u, lo) {
        out = ""
        pos++
        while (pos <= n) {
            c = substr(buf, pos, 1)
            if (c == "\"") { pos++; return out }
            if (c == "\\") {
                e = substr(buf, pos + 1, 1)
                pos += 2
                if (e == "n" || e == "r" || e == "t" || e == "b" || e == "f") out = out " "
                else if (e == "u") {
                    u = hex(substr(buf, pos, 4)); pos += 4
                    if (u >= 55296 && u < 56320 && substr(buf, pos, 2) == "\\u") {
                        lo = hex(substr(buf, pos + 2, 4))
                        if (lo >= 56320 && lo < 57344) { u = 65536 + (u - 55296) * 1024 + (lo - 56320); pos += 6 }
                    }
                    if (u < 32) out = out " "; else if (u >= 0) out = out utf8(u)
                }
                else out = out e
                continue
            }
            if (c == "\n" || c == "\t" || c == "\r") c = " "
            out = out c
            pos++
        }
        return out
    }
    { buf = buf $0 "\n" }
    END {
        n = length(buf); pos = 1; depth = 0; key = ""; expect_key = 0; inside = 0
        while (pos <= n) {
            c = substr(buf, pos, 1)
            if (c == "{" || c == "[") {
                depth++
                if (depth == 2 && c == "{" && key == want) inside = 1
                expect_key = (c == "{"); key = ""; pos++; continue
            }
            if (c == "}" || c == "]") {
                if (depth == 2) inside = 0
                depth--; pos++; continue
            }
            if (c == ",") { expect_key = 1; key = ""; pos++; continue }
            if (c == ":") { expect_key = 0; pos++; continue }
            if (c == "\"") {
                s = str()
                if (expect_key) { key = s; expect_key = 0 }
                else if (inside && depth == 2 && key != "") print key "\t" s
                continue
            }
            pos++
        }
    }' "$1"
}

xbnc_locale_file() { # CODE -> path of its file, the last directory winning
    local d f=""
    for d in "${XBNC_LOCALE_DIRS[@]}"; do
        [[ -f "$d/$1.json" ]] && f="$d/$1.json"
    done
    [[ -n "$f" ]] && echo "$f"
}

xbnc_load_texts() { # CODE
    local code="${1:-en}" f k v
    XBNC_T=()
    for f in "$(xbnc_locale_file en)" "$(xbnc_locale_file "$code")"; do
        [[ -n "$f" ]] || continue
        # An empty text in a translation means "not translated yet"; in
        # English it is a real (empty) text.
        while IFS=$'\t' read -r k v; do
            [[ -n "$k" ]] || continue
            [[ -n "$v" || ${#XBNC_T[@]} -eq 0 || -z "${XBNC_T[$k]+set}" ]] && XBNC_T["$k"]="$v"
        done < <(xbnc_json_section "$f" installer)
        if [[ "$code" == en ]]; then break; fi
    done
    return 0
}

t() { # KEY [ARG...]
    local s="$1" i=0 a
    [[ -n "${XBNC_T[$1]+set}" ]] && s="${XBNC_T[$1]}"
    shift
    for a in "$@"; do
        s="${s//\{$i\}/"$a"}"
        i=$((i + 1))
    done
    printf '%s\n' "$s"
}

# Pads a text with spaces to WIDTH characters (not bytes: "DIRECCIÓN" is 9).
pad() { # WIDTH TEXT
    local chars
    chars="$(printf '%s' "$2" | LC_ALL=C tr -d '\200-\277' | wc -c)"
    printf '%s%*s' "$2" $(( $1 > chars ? $1 - chars : 0 )) ''
}

# Output helpers of the scripts.
say() { t "$@"; }
die() { echo "Error: $(t "$@")" >&2; exit 1; }
step() { echo "==> $(t "$@")"; }
note() { # INDENT KEY [ARG...]: an indented line
    local n="$1"
    shift
    printf '%*s%s\n' "$n" '' "$(t "$@")"
}

xbnc_languages() { # "code<TAB>name<TAB>english name", English first, no duplicates
    local d f code name en seen=" "
    local -a found=()
    for d in "${XBNC_LOCALE_DIRS[@]}"; do
        for f in "$d"/*.json; do
            [[ -f "$f" ]] || continue
            code="" name="" en=""
            while IFS=$'\t' read -r k v; do
                case "$k" in code) code="$v" ;; name) name="$v" ;; english_name) en="$v" ;; esac
            done < <(xbnc_json_section "$f" language)
            [[ -n "$code" && "$code" =~ ^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})?$ && "$seen" != *" $code "* ]] || continue
            seen="$seen$code "
            found+=("$code"$'\t'"${name:-$code}"$'\t'"${en:-$name}")
        done
    done
    printf '%s\n' "${found[@]}" | awk -F'\t' '$1 == "en" { print; next } { rest = rest $0 "\n" } END { printf "%s", rest }'
}

xbnc_language_exists() { # CODE
    xbnc_languages | cut -f1 | grep -qx -- "$1"
}

# Asks for the language in a terminal: English first and by default.
xbnc_choose_language() { # prints the code chosen
    local -a codes=() lines=()
    local line i=1 reply
    while IFS= read -r line; do
        [[ -n "$line" ]] || continue
        codes+=("${line%%$'\t'*}")
        lines+=("$line")
    done < <(xbnc_languages)
    if [[ ${#codes[@]} -le 1 ]]; then echo en; return; fi
    {
        echo
        echo "Language of the installer and default language of xBNC:"
        for line in "${lines[@]}"; do
            IFS=$'\t' read -r _ name en <<<"$line"
            if [[ "$name" == "$en" || -z "$en" ]]; then printf '  %d) %s\n' "$i" "$name"; else printf '  %d) %s (%s)\n' "$i" "$name" "$en"; fi
            i=$((i + 1))
        done
        printf 'Choose [1]: '
    } >/dev/tty
    IFS= read -r reply </dev/tty || reply=""
    reply="${reply:-1}"
    if [[ "$reply" =~ ^[0-9]+$ ]] && (( reply >= 1 && reply <= ${#codes[@]} )); then
        echo "${codes[$((reply - 1))]}"
    elif printf '%s\n' "${codes[@]}" | grep -qx -- "$reply"; then
        echo "$reply"
    else
        echo en
    fi
}
