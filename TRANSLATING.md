# Translating xBNC

*The same guide in Spanish is in the wiki, section “Traducir xBNC”.*

xBNC can speak any language. English is its main language, and every text it shows lives in one file per language: the web panel, the sign-in page, everything xBNC says over IRC (`*status`, its help, notices, module messages) and the installer. You translate one file, and the language appears in every language selector of xBNC. You do not need to know how to program or to compile anything.

## The files

The files are in the `locales/` folder of the source code:

| File | What it is |
| --- | --- |
| `en.json` | English. It is the base: it holds every text of xBNC. |
| `es.json` | Spanish. |
| `<code>.json` | Any other language: `pt.json`, `fr.json`, `pt-BR.json`… |

Each file has four parts:

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

- **`language`**: `code` is the language code (two or three letters, optionally with a region: `pt`, `pt-BR`); the file must have the same name (`pt.json`). `name` is the name of the language in the language itself, as it appears in the selectors. `locale` is used for dates and numbers. `translators` is shown in `xbncd locales list`.
- **`irc`**: what xBNC says over IRC. The key on the left (`connected_to`) is a name; translate only the text on the right.
- **`panel`**: the web panel. The key on the left is the English text; translate only the text on the right.
- **`installer`**: the installer and the scripts it uses (`scripts/install.sh`, `prereqs.sh`, `update.sh`). The key on the left is a name; translate only the text on the right. A language file in `locales/` appears in the installer's language menu, and the installer installs it and makes it xBNC's default language.

## Step by step

1. Copy `en.json` to `<code>.json` (for example `pt.json`).
2. Change the `language` part: code, name, English name, locale and your name.
3. Translate the texts on the right. Never change the keys on the left.
4. Check the file:

   ```bash
   xbncd locales check pt.json             # summary and problems
   xbncd locales check pt.json --missing   # also what is left to translate
   ```

5. Try the installer in your language: put the file in `locales/` of the source code and run `sudo ./scripts/install.sh --language pt` (or choose it in the menu). Try it on a server that is already installed: copy the file to `/etc/xbnc/locales/` and reload xBNC (`sudo systemctl reload xbnc`, or `/msg *status rehash` as an administrator). The language appears straight away in the panel (sign-in page, *Your account*, *Settings*) and with `/msg *status language pt`.
6. Send it to the project (a pull request with the file in `locales/`, or through the project's contact page) so it is included in the next version.

You do not have to translate everything at once. Whatever you leave out, or leave in English, is shown in English: an incomplete translation never breaks anything. `xbncd locales check` tells you the percentage translated.

## Rules for the texts

| In the English text | What to do |
| --- | --- |
| `{0}`, `{1}`… (IRC) | Values xBNC fills in (a nick, a number…). Keep all of them; you can move them inside the sentence. |
| `{name}`, `{count}`… (panel) | The same: keep them with exactly the same name. |
| `<b>…</b>`, `<u>…</u>` (IRC) | Bold and underline in the IRC client. Keep them around the same words, each `<b>` with its `</b>`. |
| Commands and settings (`/msg *status help`, `set away delay 60`, `addserver`) | Do not translate them: they are what people type. |
| `$1`, `$nick` | Variables of the alias module: keep them as they are. |
| `Disabled\|network state` (panel key) | What follows the `\|` is a hint about where the text is shown, for languages where it matters (gender, for example). Translate only the text, without the hint. |
| Line breaks | Not allowed in IRC or installer texts. |

A text that breaks these rules (missing placeholders, a `<b>` without `</b>`, line breaks) is ignored and shown in English; `xbncd locales check` lists them as problems.

Plurals: words that change with a number are separate texts ending in `.one` and `.other` (`unit.network.one` = `{0} network`, `unit.network.other` = `{0} networks`). `.one` is used for 1 and `.other` for everything else.

## Correcting a built-in language

A file for a language that already exists (for example `es.json` in `/etc/xbnc/locales/`) only needs the texts you want to change: the rest stays as it is. This is useful to try out corrections before sending them.

## What a file does not translate

- **Plugin replies.** The name, description and settings of a plugin can be translated in its manifest (`translations`). What a plugin answers in a channel (the trivia questions, for example) is written in the plugin itself; in a language the plugin does not include, it answers in English. The official plugins speak English and Spanish.
- **Log messages** in the server log are in English.

## For developers

- In the panel, write texts in English inside `t("...")`, `tk("...")` or `tx("...")`. Then run `npm run i18n:check` in `web/` (it lists the texts missing from `en.json`) and `node scripts/i18n-check.mjs --sync` to add them.
- In the installer scripts, write `say KEY`, `step KEY`, `die KEY`, `note 4 KEY` or `box KEY` with the arguments after the key (`step writing "$CONFIG"`), and add the key with its English text to the `installer` section of `locales/en.json` (and the Spanish one to `es.json`). The texts are read by `scripts/i18n.sh` with bash and awk only. The tests check that every key the scripts use exists and that every installer text is used.
- In the daemon, add the text to the `irc` section of `locales/en.json` and use it with `self.t("key", &[])`, `tr!(self, "key", arg1, arg2)` or `i18n::tr(lang, "key", &[...])`. The tests check that every key used exists and that `es.json` translates all of them with the same placeholders.

