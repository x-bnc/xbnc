<a id="translations"></a>

# Translating xBNC

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

xBNC can speak any language. Every text it shows lives in one file per language: the web panel, the sign-in page and everything it says over IRC (`*status`, its help, notices and module messages). You translate one file and the language appears in every language selector of xBNC, without programming or compiling anything.

### The files

| File | What it is |
| --- | --- |
| `locales/en.json` | English. It is the base: it holds every text of xBNC. |
| `locales/es.json` | Spanish. |
| `locales/<code>.json` | Any other language: `pt.json`, `fr.json`, `pt-BR.json`… |

English and Spanish are built into the program. Other languages, and corrections, go in `/etc/xbnc/locales/` (`locales_dir` in [xbnc.toml](CONFIGURATION.md#configuration), or the `XBNC_LOCALES_DIR` variable). xBNC reads them at start and on reload (`sudo systemctl reload xbnc` or `/msg *status rehash`).

```json
{
  "language": {
    "code": "pt",
    "name": "Português",
    "english_name": "Portuguese",
    "locale": "pt-PT",
    "translators": ["Your name or nick"]
  },
  "irc": {
    "connected_to": "Ligado a {0} como {1}.",
    "help.group.channels": "Canais"
  },
  "panel": {
    "Joined": "Dentro",
    "Delete {username}": "Apagar {username}"
  },
  "installer": {
    "finding_the_program": "A procurar o programa",
    "installing_the_binary_to": "A instalar o programa em {0}"
  }
}
```

| Part | What it is |
| --- | --- |
| `language` | `code`: the language code (two or three letters, with a region if needed: `pt`, `pt-BR`); the file has the same name. `name`: the name of the language in the language itself, as shown in the selectors. `locale`: for dates and numbers. `translators`: shown by `xbncd locales list`. |
| `irc` | What xBNC says over IRC. The key on the left is a name; translate only the text on the right. |
| `panel` | The web panel. The key on the left is the English text; translate only the text on the right. |
| `installer` | The installer and the scripts it uses. The key on the left is a name; translate only the text on the right. A language file in `locales/` of the source code appears in the installer's language menu; the installer installs it and makes it xBNC's default language. |

### Step by step

1. Copy `en.json` as `<code>.json` (for example `pt.json`).
2. Change the `language` part: code, name, English name, locale and your name.
3. Translate the texts on the right. Never change the keys on the left.
4. Check the file: `xbncd locales check pt.json` (add `--missing` to see what is left).
5. Try the installer in your language: put the file in `locales/` of the source code and run `sudo ./scripts/install.sh --language pt`, or choose it in the menu.
6. Try it on a server already installed: copy it to `/etc/xbnc/locales/` and reload xBNC. It appears straight away on the sign-in page, in *Your account*, in *Settings* and with `/msg *status language pt`.
7. Send it to the project so it is included in the next version.

> You do not have to translate everything at once. Whatever you leave out is shown in English: an incomplete translation never breaks anything. `xbncd locales check` tells you the share translated.

### Rules for the texts

| In the English text | What to do |
| --- | --- |
| `{0}`, `{1}`… (IRC) | Values xBNC fills in (a nick, a number…). Keep all of them; you can move them inside the sentence. |
| `{name}`, `{count}`… (panel) | The same: keep them with exactly the same name. |
| `<b>…</b>`, `<u>…</u>` (IRC) | Bold and underline in the IRC client. Keep them around the same words, each `<b>` with its `</b>`. |
| Commands and settings (`/msg *status help`, `set away delay 60`) | Do not translate them: they are what people type. |
| `$1`, `$nick` | Variables of the alias module: keep them as they are. |
| `Disabled\|network state` (panel key) | What follows the `\|` is a hint about where the text is shown (for gender, for example). Translate only the text, without the hint. |
| Plurals | Texts ending in `.one` are used with 1 and those ending in `.other` with any other number (`unit.network.one` = “{{0}} network”). |
| Line breaks | Not allowed in IRC or installer texts. |

A text that breaks these rules is ignored and shown in English; `xbncd locales check` reports it as a problem. This also makes it impossible for a language file to inject IRC commands (line breaks are forbidden).

### Correcting a built-in language

A file for a language that already exists (for example `es.json` in `/etc/xbnc/locales/`) only needs the texts you want to change: the rest stays as it is.

### What a file does not translate

- **Plugin replies.** Their name, description and settings are translated in their manifest (`translations`). What a plugin answers in a channel is written in the plugin itself; in a language it does not include, it answers in English. The official plugins speak English and Spanish.
- **The server log** is in English.

### Translating this wiki

The wiki is a project of its own, separate from the app (`xbnc-wiki`), so that anyone can read it without installing xBNC. It is generated with `build.py` from one content file per language (`content/en.py` is the base) and the xBNC source code. To translate it, copy `en.py` as `<code>.py` and translate the texts between quotes without touching the HTML tags or calls such as `code(…)`. The tables of modules, plugins and commands translate themselves with xBNC's language files. When the wiki is built, each language appears in the selector at the top.

### For developers

- In the panel, texts are written in English inside `t("…")`, `tk("…")` or `tx("…")`. `npm run i18n:check` (in `web/`) lists what is missing from `en.json`, and `node scripts/i18n-check.mjs --sync` adds it.
- In the server, the text is added to the `irc` section of `locales/en.json` and used with `self.t("key", &[])`, `tr!(self, "key", arg1, arg2)` or `i18n::tr(lang, "key", &[…])`. The tests check that every key used exists and that `es.json` translates all of them with the same placeholders.

The complete guide is in `TRANSLATING.md`, in the source code.
