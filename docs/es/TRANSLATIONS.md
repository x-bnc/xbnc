<a id="translations"></a>

# Traducir xBNC

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

xBNC puede hablar cualquier idioma. Todos sus textos están en un fichero por idioma: el panel web, la pantalla de entrada y todo lo que dice por IRC (`*status`, su ayuda, los avisos y los mensajes de los módulos). Se traduce un fichero y el idioma aparece en todos los selectores de idioma de xBNC, sin programar ni compilar nada.

### Los ficheros

| Fichero | Qué es |
| --- | --- |
| `locales/en.json` | Inglés. Es la base: tiene todos los textos de xBNC. |
| `locales/es.json` | Español. |
| `locales/<código>.json` | Cualquier otro idioma: `pt.json`, `fr.json`, `pt-BR.json`… |

El inglés y el español van dentro del programa. Los demás idiomas, y las correcciones, se ponen en `/etc/xbnc/locales/` (`locales_dir` en [xbnc.toml](CONFIGURATION.md#configuration), o la variable `XBNC_LOCALES_DIR`). xBNC los lee al arrancar y al recargar (`sudo systemctl reload xbnc` o `/msg *status rehash`).

```json
{
  "language": {
    "code": "pt",
    "name": "Português",
    "english_name": "Portuguese",
    "locale": "pt-PT",
    "translators": ["Tu nombre o nick"]
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

| Parte | Qué es |
| --- | --- |
| `language` | `code`: código del idioma (dos o tres letras, con región si hace falta: `pt`, `pt-BR`); el fichero se llama igual. `name`: nombre del idioma en ese idioma, como sale en los selectores. `locale`: para fechas y números. `translators`: sale en `xbncd locales list`. |
| `irc` | Lo que xBNC dice por IRC. La clave de la izquierda es un nombre; solo se traduce el texto de la derecha. |
| `panel` | El panel web. La clave de la izquierda es el texto en inglés; solo se traduce el texto de la derecha. |
| `installer` | El instalador y los scripts que usa. La clave de la izquierda es un nombre; solo se traduce el texto de la derecha. Un fichero de idioma en `locales/` del código fuente aparece en el menú de idiomas del instalador; el instalador lo instala y lo deja como idioma predeterminado de xBNC. |

### Paso a paso

1. Copia `en.json` como `<código>.json` (por ejemplo, `pt.json`).
2. Cambia la parte `language`: código, nombre, nombre en inglés, locale y tu nombre.
3. Traduce los textos de la derecha. No cambies nunca las claves de la izquierda.
4. Comprueba el fichero: `xbncd locales check pt.json` (añade `--missing` para ver lo que falta).
5. Prueba el instalador en tu idioma: pon el fichero en `locales/` del código fuente y ejecuta `sudo ./scripts/install.sh --language pt`, o elígelo en el menú.
6. Pruébalo en un servidor ya instalado: cópialo en `/etc/xbnc/locales/` y recarga xBNC. Aparece al momento en la pantalla de entrada, en *Tu cuenta*, en *Ajustes* y con `/msg *status language pt`.
7. Envíalo al proyecto para incluirlo en la siguiente versión.

> No hace falta traducirlo todo de una vez. Lo que no traduzcas se muestra en inglés: una traducción a medias nunca rompe nada. `xbncd locales check` dice el porcentaje traducido.

### Reglas de los textos

| En el texto en inglés | Qué hacer |
| --- | --- |
| `{0}`, `{1}`… (IRC) | Valores que pone xBNC (un nick, un número…). Mantenlos todos; puedes moverlos dentro de la frase. |
| `{name}`, `{count}`… (panel) | Igual: mantenlos con el mismo nombre exacto. |
| `<b>…</b>`, `<u>…</u>` (IRC) | Negrita y subrayado en el cliente IRC. Mantenlos alrededor de las mismas palabras, cada `<b>` con su `</b>`. |
| Comandos y ajustes (`/msg *status help`, `set away delay 60`) | No se traducen: es lo que la gente escribe. |
| `$1`, `$nick` | Variables del módulo de alias: se dejan tal cual. |
| `Disabled\|network state` (clave del panel) | Lo que va tras la `\|` es una pista de dónde sale el texto (para el género, por ejemplo). Se traduce solo el texto, sin la pista. |
| Plurales | Los textos terminados en `.one` se usan con 1 y los terminados en `.other` con cualquier otro número (`unit.network.one` = «{{0}} red»). |
| Saltos de línea | No se admiten en los textos de IRC ni del instalador. |

Un texto que no cumple estas reglas se ignora y sale en inglés; `xbncd locales check` lo indica como problema. Esto impide también que un fichero de idioma pueda meter comandos IRC (los saltos de línea están prohibidos).

### Corregir un idioma incluido

Un fichero de un idioma que ya existe (por ejemplo, `es.json` en `/etc/xbnc/locales/`) solo necesita los textos que quieras cambiar: el resto se queda como está.

### Lo que no se traduce con un fichero

- **Las respuestas de los plugins.** Su nombre, descripción y ajustes se traducen en su manifiesto (`translations`). Lo que un plugin responde en un canal está escrito en el propio plugin; en un idioma que no incluye, responde en inglés. Los plugins oficiales hablan inglés y español.
- **El registro del servidor** está en inglés.

### Traducir esta wiki

La wiki es un proyecto propio, aparte de la app (`xbnc-wiki`), para que cualquiera pueda leerla sin instalar xBNC. Se genera con `build.py` a partir de un fichero de contenido por idioma (`content/en.py` es la base) y del código fuente de xBNC. Para traducirla, copia `en.py` como `<código>.py` y traduce los textos entre comillas sin tocar las etiquetas HTML ni las llamadas como `code(…)`. Las tablas de módulos, plugins y comandos se traducen solas con los ficheros de idioma de xBNC. Al generar la wiki, cada idioma aparece en el selector de la cabecera.

### Para desarrolladores

- En el panel, los textos se escriben en inglés dentro de `t("…")`, `tk("…")` o `tx("…")`. `npm run i18n:check` (en `web/`) lista lo que falta en `en.json`, y `node scripts/i18n-check.mjs --sync` lo añade.
- En el servidor, el texto se añade a la sección `irc` de `locales/en.json` y se usa con `self.t("clave", &[])`, `tr!(self, "clave", arg1, arg2)` o `i18n::tr(lang, "clave", &[…])`. Los tests comprueban que cada clave usada existe y que `es.json` las traduce todas con los mismos marcadores.

La guía completa, en inglés, está en `TRANSLATING.md`, en el código fuente; esta sección es su versión en español.
