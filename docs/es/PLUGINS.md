# Plugins

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

- [Plugins](#plugins)
- [Escribir plugins](#plugin-development)

<a id="plugins"></a>

## Plugins

Los plugins añaden funciones para todo el canal. Se ejecutan en **WebAssembly, aislados**: no pueden leer ficheros ni ver otras redes, tienen límites de CPU y memoria, y solo hacen lo que permiten sus permisos, que se ven antes de instalarlos. Los que salen a internet solo pueden conectarse a los sitios aprobados.

1. Los **18 plugins oficiales vienen instalados**: xBNC instala los que faltan al arrancar por primera vez y después de cada actualización, y actualiza los que tengan una versión antigua (o la misma versión con un arreglo). Quedan desactivados en todas las redes. Uno que un superadministrador desinstala a propósito no vuelve hasta la siguiente versión de xBNC (o hasta que se instala de nuevo desde *Plugins → Catálogo*).
2. Un **superadministrador** puede instalar otros desde el catálogo remoto o subiendo un fichero `.wasm`, y aprueba los sitios web de los plugins que salen a internet («Sitios a los que puede conectarse»). Los de los oficiales (títulos de enlaces, el tiempo y noticias) vienen aprobados, y ahí se puede bloquear cualquiera.
3. Cuando el catálogo oficial firmado está activo, xBNC lo consulta una vez al día y actualiza los plugins instalados que tengan allí una versión más nueva (conservan sus ajustes). Antes de instalar nada se comprueban la firma del índice (Ed25519) y el SHA-256 de cada fichero.
4. Cada usuario los activa en sus redes desde *Módulos*, en la sección Plugins, y ajusta su configuración.
5. Contestan en tu idioma de xBNC (o en el del ajuste «Idioma») y funcionan también con tus propios mensajes. Los oficiales hablan inglés y español; en otro idioma responden en inglés.

| Permiso | Qué permite |
| --- | --- |
| `send` | Enviar mensajes y avisos. |
| `raw` | Comandos IRC como KICK, MODE o INVITE. Nunca QUIT, PASS, USER, NICK, CAP, AUTHENTICATE, OPER, SERVER ni SQUIT. |
| `timer` | Recibir un evento cada minuto (o cada 5–60 segundos si el plugin lo pide). |
| `presence` | Ver entradas, salidas, kicks, cambios de nick y la lista de usuarios de los canales. |
| `http` | Pedir páginas https, solo a los dominios que declara (hasta 10) y que apruebe el superadministrador. |

> Escribe siempre algo después del nombre: `/msg *poll ayuda`. Algunos clientes, como HexChat, muestran su propia ayuda de /msg si no escribes nada detrás.

<a id="plugin-trivia"></a>

### Trivial

`trivia 1.0.0` · *nuevo en 1.0*

Juego de preguntas para tus canales, con pistas, puntos y ranking.

**Cómo se usa**

En un canal: !trivial para empezar, !trivial stop y !trivial top. Se responde escribiendo en el canal. /msg \*trivia add ¿Pregunta? \| respuesta añade preguntas propias; list y del N para gestionarlas.

**Ejemplo**

```text
<carol> !trivial
<y2k> ¡Empieza el trivial! 10 preguntas; responde escribiendo en el canal.
<y2k> Pregunta 1/10: ¿Cuál es la capital de Japón?
<bob> tokio
<y2k> ¡Correcto, bob! Era «Tokio». Llevas 1 punto.
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Comando (`trigger`) | !trivia |   |
| Preguntas por partida (`rounds`) | 10 |   |
| Segundos para responder (`seconds`) | 40 | A mitad de tiempo sale una pista. |
| Solo en estos canales (`channels`) | vacío | Uno por línea. Vacío: en todos. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes, temporizador. Ayuda desde IRC: `/msg *trivia ayuda`

<a id="plugin-greet"></a>

### Bienvenida

`greet 1.0.0` · *nuevo en 1.0*

Saluda a quien entra en tus canales, con un mensaje distinto la primera vez y saludos personales.

**Cómo se usa**

Automático. /msg \*greet test #canal nick muestra qué se diría. En los mensajes se cambian {nick} y {channel}.

**Ejemplo**

```text
* carol ha entrado en #xbnc
<y2k> ¡Bienvenido a #xbnc, carol! Es tu primera vez por aquí.
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| La primera vez (`first`) | ¡Bienvenido a {channel}, {nick}! Es tu primera vez por aquí. | Para quien nunca ha estado en el canal. Se cambian {nick} y {channel}. |
| Cuando vuelven (`message`) | Hola {nick}, bienvenido de nuevo a {channel}. | Vacío: no se saluda a quien vuelve. |
| Saludos personales (`personal`) | vacío | Uno por línea: nick y mensaje. Sustituye al habitual para esa persona. |
| Saludar en privado (notice) (`notice`) | no | Desactivado: el saludo se escribe en el canal. |
| Minutos antes de volver a saludar a la misma persona (`cooldown`) | 30 |   |
| No saludar nunca (`ignore`) | vacío | Nicks, uno por línea (bots, servicios). |
| Solo en estos canales (`channels`) | vacío | Uno por línea. Vacío: en todos. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes, entradas y salidas. Ayuda desde IRC: `/msg *greet ayuda`

<a id="plugin-urltitle"></a>

### Títulos de enlaces

`urltitle 1.0.0` · *nuevo en 1.0*

Muestra el título de los enlaces de YouTube, Vimeo, SoundCloud, Dailymotion y Spotify, artículos de Wikipedia y repositorios de GitHub que se pegan en tus canales.

**Cómo se usa**

Automático. Un administrador puede bloquear cualquiera de estos sitios en el panel (Plugins → «Sitios a los que puede conectarse»).

**Ejemplo**

```text
<bob> mirad esto https://www.youtube.com/watch?v=…
<y2k> YouTube: Cómo montar un bouncer IRC en 5 minutos
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Segundos entre títulos en un canal (`cooldown`) | 5 |   |
| También para tus propios enlaces (`own`) | sí |   |
| Solo en estos canales (`channels`) | vacío | Uno por línea. Vacío: en todos. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes, acceso web a sitios aprobados. Ayuda desde IRC: `/msg *urltitle ayuda`

<a id="plugin-factoids"></a>

### Definiciones

`factoids 1.0.0` · *nuevo en 1.0*

La memoria del canal: respuestas cortas a preguntas frecuentes que tú y tus editores mantenéis al día.

**Cómo se usa**

En un canal: !def palabra (o ?palabra), !def palabra = texto, !def -palabra, !defs. Aquí: /msg \*factoids palabra.

**Ejemplo**

```text
<y2k> !def normas = Sé respetuoso y no hagas flood.
<carol> ?normas
<y2k> normas: Sé respetuoso y no hagas flood.
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Comando (`trigger`) | !def |   |
| Responder también a ?palabra (`question_mark`) | sí |   |
| Quién más puede cambiar definiciones (`editors`) | vacío | Nicks, uno por línea. Tú siempre puedes. |
| Solo en estos canales (`channels`) | vacío | Uno por línea. Vacío: en todos. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes. Ayuda desde IRC: `/msg *factoids ayuda`

<a id="plugin-weather"></a>

### El tiempo

`weather 1.0.0` · *nuevo en 1.0*

El tiempo ahora y la previsión de hoy para cualquier ciudad, de Open-Meteo (gratis, sin clave).

**Cómo se usa**

En un canal: !tiempo Madrid o !weather London. Aquí: /msg \*weather Sevilla.

**Ejemplo**

```text
<bob> !tiempo Sevilla
<y2k> Sevilla (España): 22 °C, cielo despejado, viento 9 km/h, humedad 40 %. Hoy entre 14 y 25 °C.
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Comando (`trigger`) | !weather |   |
| Ciudad por defecto (`city`) | vacío | Se usa cuando no se escribe ninguna. |
| Solo en estos canales (`channels`) | vacío | Uno por línea. Vacío: en todos. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes, acceso web a sitios aprobados. Ayuda desde IRC: `/msg *weather ayuda`

<a id="plugin-news"></a>

### Noticias

`news 1.0.0` · *nuevo en 1.0*

Publica en tus canales las novedades de fuentes RSS y Atom (periódicos, blogs, canales de YouTube, versiones de GitHub).

**Cómo se usa**

Configura las fuentes en el panel: una por línea, «#canal https://...». /msg \*news list, /msg \*news check.

**Ejemplo**

```text
<y2k> [Hacker News] Show HN: un bouncer IRC escrito en Rust — https://…
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Fuentes (`feeds`) | vacío | Una por línea: canal y dirección, por ejemplo #xbnc https://hnrss.org/frontpage. Solo estos sitios: YouTube, GitHub, Reddit, Hacker News (hnrss.org), BBC, El País, RTVE, WordPress, Blogspot y Substack. |
| Minutos entre comprobaciones (`interval`) | 15 |   |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes, temporizador, acceso web a sitios aprobados. Ayuda desde IRC: `/msg *news ayuda`

<a id="plugin-karma"></a>

### Karma

`karma 1.0.0` · *nuevo en 1.0*

Puntos entre personas: nick++ y nick--, con ranking.

**Cómo se usa**

En un canal: nick++, nick--, !karma nick, !karma top. Aquí: /msg \*karma nick.

**Ejemplo**

```text
<bob> carol++
<y2k> carol tiene 1 de karma.
<carol> !karma top
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Solo en estos canales (`channels`) | vacío | Uno por línea. Vacío: en todos. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes. Ayuda desde IRC: `/msg *karma ayuda`

<a id="plugin-quotes"></a>

### Citas

`quotes 1.0.0` · *nuevo en 1.0*

Frases memorables del canal: guárdalas, búscalas y saca una al azar.

**Cómo se usa**

En un canal: !cita, !cita 12, !cita buscar palabra, !cita añadir texto, !cita borrar 12 (o !quote add/search/del).

**Ejemplo**

```text
<bob> !cita añadir hoy es un gran día
<y2k> Cita 1 guardada.
<carol> !cita 1
<y2k> #1: hoy es un gran día
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Comando (`trigger`) | !quote |   |
| Cualquiera puede añadir citas (`anyone_adds`) | sí | Desactivado: solo tú y los editores. |
| Editores (`editors`) | vacío | Nicks que pueden añadir y borrar citas, uno por línea. |
| Solo en estos canales (`channels`) | vacío | Uno por línea. Vacío: en todos. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes. Ayuda desde IRC: `/msg *quotes ayuda`

<a id="plugin-badwords"></a>

### Palabras prohibidas

`badwords 1.0.0` · *nuevo en 1.0*

Avisa, echa y al final banea a quien escribe palabras prohibidas en tus canales (necesitas op).

**Cómo se usa**

Pon las palabras en el panel. /msg \*badwords test texto dice si un texto se detectaría.

**Ejemplo**

```text
<carol> eres tontito
<y2k> carol: esa palabra no está permitida aquí. La próxima vez, kick.
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Palabras prohibidas (`words`) | vacío | Una por línea. Da igual mayúsculas y tildes; un \* al final vale por el principio de una palabra. |
| Duración del ban (minutos) (`ban_minutes`) | 10 | 0: el ban se queda. |
| Motivo del kick (`reason`) | vacío |   |
| Avisar en privado (notice) (`warn_private`) | no |   |
| No actuar nunca sobre (`exempt`) | vacío | Nicks, uno por línea. |
| Solo en estos canales (`channels`) | vacío | Uno por línea. Vacío: en todos. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes, comandos IRC (kick, ban), temporizador. Ayuda desde IRC: `/msg *badwords ayuda`

<a id="plugin-games"></a>

### Juegos

`games 1.0.0` · *nuevo en 1.0*

Ahorcado, bola 8 mágica y piedra, papel o tijera para tus canales.

**Cómo se usa**

En un canal: !8ball pregunta, !ppt piedra (o !rps rock), !ahorcado (o !hangman) y !a letra para jugar.

**Ejemplo**

```text
<bob> !ppt piedra
<y2k> piedra contra tijera: ¡ganas, bob!
<bob> !8ball ¿funcionará?
<y2k> La bola 8 dice: «Pregunta más tarde».
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Solo en estos canales (`channels`) | vacío | Uno por línea. Vacío: en todos. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes. Ayuda desde IRC: `/msg *games ayuda`

<a id="plugin-dice"></a>

### Dados

`dice 1.1.0`

Tira dados en canales y privados: !dado, !dado 2d6, !dado d20.

**Cómo se usa**

En un canal o privado: !dado (un dado de seis caras), !dado 2d6, !dado d20. También !roll y !dice.

/msg \*dice 3d8 tira solo para ti.

**Ejemplo**

```text
<bob> !dado 2d6
<y2k> bob tira 2 dados de 6 caras: 3 y 5 (total 8).
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Comando adicional (`trigger`) | !roll | !dado, !dados, !roll y !dice funcionan siempre. |
| Máximo de dados por tirada (`max_dice`) | 10 |   |
| Responder en privados (`in_queries`) | sí |   |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes. Ayuda desde IRC: `/msg *dice ayuda`

<a id="plugin-poll"></a>

### Encuestas

`poll 1.1.0`

!poll ¿Pregunta? \| opción \| opción, y luego !vote N. Se cierra sola y anuncia la ganadora.

**Cómo se usa**

En un canal: !poll ¿Pizza o sushi? \| pizza \| sushi (hasta 10 opciones) — vota con !vote 1 — !poll muestra el recuento — !poll end la cierra (quien la creó, o tú). Se cierra sola pasados los minutos de los ajustes. También !encuesta y !votar.

**Ejemplo**

```text
<carol> !poll ¿Café o té? | café | té
<bob> !vote 2
<*poll> Voto registrado en #xbnc: 2) té. Puedes cambiarlo votando otra vez.
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Comando (`trigger`) | !poll |   |
| Comando para votar (`vote_trigger`) | !vote |   |
| Minutos hasta que se cierra (`minutes`) | 60 |   |
| Otras personas pueden crear encuestas (`public`) | sí | Desactivado: solo las creas tú; votar puede todo el mundo. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes, temporizador. Ayuda desde IRC: `/msg *poll ayuda`

<a id="plugin-remind"></a>

### Recordatorios

`remind 1.1.0`

Te lo recuerda (a ti o a cualquiera del canal) pasado un tiempo o a una hora: !recuerda 10m sacar la pizza, !recuerda 18:30 reunión.

**Cómo se usa**

En un canal: !recuerda 10m sacar la pizza — !recuerda 2 horas llamar a Bob — !recuerda 1h30m … — !recuerda 18:30 reunión — !recuerda lista — !recuerda borrar. También !remind.

/msg \*remind 10m … te lo recuerda en privado. Los recordatorios se comprueban cada minuto.

**Ejemplo**

```text
<bob> !recuerda en 10 minutos sacar la pizza
<y2k> bob: vale, te lo recuerdo dentro de 10 minutos.
<y2k> bob: sacar la pizza
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Comando adicional (`trigger`) | !remind | !remind y !recuerda funcionan siempre. |
| Otras personas pueden crear recordatorios (`public`) | sí | Desactivado: solo los tuyos y /msg \*remind. |
| Recordatorios pendientes por persona (`max_per_nick`) | 10 |   |
| Zona horaria para horas como 18:30 (`timezone`) | Europe/Madrid | Europe/Madrid, Europe/London, UTC o una diferencia como +1. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes, temporizador. Ayuda desde IRC: `/msg *remind ayuda`

<a id="plugin-tell"></a>

### Mensajes diferidos

`tell 1.1.0`

!tell nick mensaje: se lo da en el canal cuando esa persona escriba o entre.

**Cómo se usa**

En un canal: !tell bob el build está arreglado — bob lo recibe en el canal en cuanto escriba algo o entre. Funciona para ti y para cualquiera del canal. También !avisa.

**Ejemplo**

```text
<y2k> !tell carol el build está arreglado
<y2k> y2k: vale, se lo diré a carol en cuanto escriba o entre en el canal.
* carol ha entrado en #xbnc
<y2k> carol: y2k te dejó un mensaje hace 5 minutos: el build está arreglado
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Comando (`trigger`) | !tell |   |
| Entregar al entrar (`on_join`) | sí | Desactivado: solo cuando escriba algo. |
| Mensajes esperando por persona (`max_per_nick`) | 5 |   |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes, entradas y salidas. Ayuda desde IRC: `/msg *tell ayuda`

<a id="plugin-autoresponder"></a>

### Respuestas automáticas

`autoresponder 1.1.0`

Responde solo cuando alguien escribe una palabra clave y, si quieres, a los privados mientras estás ausente.

**Cómo se usa**

Escribe las reglas en los ajustes, una por línea: palabra = respuesta. {nick} es quien escribió y {channel} el canal. La regla \* = texto responde a cualquier privado.

/msg \*autoresponder muestra las reglas; /msg \*autoresponder test \<texto\> las prueba.

**Ejemplo**

```text
<bob> hola, ¿alguien?
<y2k> ¡Hola bob! Ahora mismo no estoy, déjame tu mensaje.
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Reglas (`rules`) | vacío | Una por línea: palabra = respuesta. Ejemplo: hola = ¡Hola {nick}! — \* = respuesta a cualquier privado. |
| Segundos antes de que la misma regla vuelva a responder (`cooldown`) | 60 |   |
| Responder también a privados (`in_queries`) | sí |   |
| Solo cuando no hay ningún cliente conectado (`only_when_away`) | no | Así responde por ti mientras estás ausente. |
| Solo en estos canales (`channels`) | vacío | Uno por línea. Vacío: en todos. |
| Idioma de los mensajes (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes. Ayuda desde IRC: `/msg *autoresponder ayuda`

<a id="plugin-chanstats"></a>

### Estadísticas del canal

`chanstats 1.1.0`

!stats: mensajes del canal y quién habla más. !stats nick: lo de una persona.

**Cómo se usa**

En un canal: !stats muestra los mensajes y quién habla más — !stats bob, lo de una persona. /msg \*chanstats #canal te lo muestra en privado. Los recuentos se guardan cada minuto. También !estadisticas y !top.

**Ejemplo**

```text
<bob> !stats
<y2k> #xbnc: 1204 mensajes desde el 3 nov 2026. Más activos: carol 312, bob 280, y2k 199.
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Comando (`trigger`) | !stats |   |
| Personas en la clasificación (`top`) | 5 |   |
| Responder a otras personas (`public`) | sí | Desactivado: solo tu propio !stats. |
| Solo en estos canales (`channels`) | vacío | Uno por línea. Vacío: en todos. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes, temporizador. Ayuda desde IRC: `/msg *chanstats ayuda`

<a id="plugin-seen"></a>

### Visto por última vez

`seen 2.1.0`

!seen nick: cuándo se vio a alguien por última vez y qué hacía, o en qué canales está ahora.

**Cómo se usa**

En un canal: !seen bob — cuándo se vio a bob por última vez y si está ahora. /msg \*seen bob, o /msg \*seen stats.

**Ejemplo**

```text
<bob> !seen carol
<y2k> carol fue visto por última vez hace 2 horas, en #xbnc diciendo «me voy a cenar».
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Comando (`trigger`) | !seen | Lo que se escribe antes del nick. |
| Responder a otras personas en los canales (`public`) | sí | Desactivado: solo te responde a ti (tu propio !seen y /msg \*seen). |
| Segundos entre respuestas en un canal (`cooldown`) | 5 |   |
| Mostrar lo último que dijo (`show_text`) | sí | Desactivado: solo dice dónde y cuándo. |
| Responder solo en estos canales (`channels`) | vacío | Uno por línea. Vacío: en todos. La actividad se registra en todos. |
| Idioma de las respuestas (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: enviar mensajes, entradas y salidas. Ayuda desde IRC: `/msg *seen ayuda`

<a id="plugin-antiflood"></a>

### Anti-flood

`antiflood 1.1.0`

Avisa, expulsa o banea a quien hace flood o repite mensajes en tus canales (tienes que ser operador del canal).

**Cómo se usa**

Funciona solo, en los canales que pongas en los ajustes (tienes que ser operador allí). /msg \*antiflood muestra cómo está configurado.

**Ejemplo**

```text
<spam> a
<spam> a
<spam> a
* spam ha sido expulsado de #xbnc por y2k (flood)
```

| Ajuste | Por defecto | Detalle |
| --- | --- | --- |
| Canales que proteger (`channels`) | vacío | Uno por línea. No hace nada hasta que pongas alguno. |
| Máximo de líneas (`lines`) | 5 |   |
| En estos segundos (`seconds`) | 4 |   |
| Misma línea repetida (`repeats`) | 3 | Veces seguidas; 0 lo desactiva. |
| Qué hacer (`action`) | kick | warn (avisar la primera vez y expulsar la segunda), kick o ban. |
| Duración del ban en minutos (`ban_minutes`) | 10 | 0: el ban no se quita. |
| Motivo de la expulsión (`reason`) | vacío |   |
| No actuar nunca contra estos nicks (`exempt`) | vacío | Uno por línea. |
| Idioma de los mensajes (`lang`) | vacío | Vacío: tu idioma en xBNC. O es / en. |

Permisos: comandos IRC (kick, ban), enviar mensajes, temporizador. Ayuda desde IRC: `/msg *antiflood ayuda`

<a id="plugin-development"></a>

## Escribir plugins

Un plugin es un fichero WebAssembly con un manifiesto JSON dentro. Hay SDK para **C** (`sdk/c`, con los 18 plugins oficiales como ejemplo y el ayudante `xbnc_util.h`) y para **Rust** (`sdk/rust`). Cualquier lenguaje que compile a WebAssembly sin WASI sirve si cumple la interfaz.

### Manifiesto

| Campo | Obligatorio | Qué es |
| --- | --- | --- |
| `id` | sí | 1–32 caracteres a-z 0-9 \_ -, empezando por letra. No puede ser status, notes, watch, events, xbnc, bnc, plugin ni plugins. Es el nombre de su servicio (\*id). |
| `name` | sí | Nombre visible, 1–60 caracteres. |
| `version` | sí | Versión, 1–20 caracteres. |
| `description` | no | Qué hace. |
| `usage` | no | Cómo se usa (hasta 1000 caracteres). El panel lo muestra en «Cómo se usa» y xBNC lo responde si el plugin no contesta a un comando. |
| `author` | no | Autor. |
| `homepage` | no | Página del plugin (https://). |
| `abi` | no | Versión de la interfaz (1). |
| `permissions` | no | Lista de send, raw, timer, presence y http. |
| `http_domains` | con http | De 1 a 10 dominios a los que pide permiso para conectarse. |
| `timer_secs` | no | Con timer: recibir el temporizador cada 5–60 segundos en lugar de cada minuto. |
| `settings` | no | Hasta 20 ajustes: {key, type (bool, int, text o lines), label, help, default, min, max}. El panel los muestra como formulario. |
| `translations` | no | Por código de idioma (es, pt, fr…): name, description, usage y, por ajuste, label y help. Los textos de la raíz del manifiesto son la base, en inglés. |

```json
{
  "id": "hola",
  "name": "Hello",
  "version": "1.0.0",
  "description": "Answers !hola.",
  "usage": "In a channel: !hola",
  "author": "tu-nombre",
  "permissions": ["send"],
  "settings": [
    { "key": "greeting", "type": "text", "label": "Greeting", "default": "Hello!" },
    { "key": "lang", "type": "text", "label": "Language", "help": "Empty: your xBNC language.", "default": "" }
  ],
  "translations": {
    "es": {
      "name": "Hola",
      "description": "Responde a !hola.",
      "usage": "En un canal: !hola",
      "settings": { "greeting": { "label": "Saludo" } }
    }
  }
}
```

### Interfaz (ABI 1)

El módulo exporta:

| Export | Firma | Qué hace |
| --- | --- | --- |
| `memory` | memoria | Memoria lineal del plugin. |
| `xbnc_alloc` | `(len: i32) -> i32` | Reserva len bytes para el evento y devuelve su dirección. |
| `xbnc_on_event` | `(ptr: i32, len: i32) -> i64` | Recibe el evento (JSON UTF-8) y devuelve (dirección \<\< 32) \| longitud de la respuesta (JSON), o 0 si no responde. |

Y solo puede importar del módulo `xbnc` (ni WASI ni nada más):

| Import | Qué hace |
| --- | --- |
| `log(ptr, len)` | Escribe una línea en el registro (máximo 10 por evento y 500 caracteres). |
| `now_ms() -> i64` | Milisegundos desde 1970. |
| `kv_get`, `kv_set`, `kv_del`, `kv_list` | Almacén clave-valor propio del plugin en esa red: claves de hasta 200 bytes, valores de hasta 64 KB, 16 MB y 200 000 claves en total, hasta 2 000 escrituras por evento (se guardan juntas). |

### Eventos

```json
{
  "event": "message",
  "network": "ejemplo", "nick": "y2k", "connected": true, "clients": 1, "lang": "es",
  "config": { "trigger": "!dado" },
  "state": null,
  "command": "PRIVMSG", "conversation": "#xbnc", "channel": true,
  "from": "bob", "from_mask": "bob!b@host", "text": "!dado 2d6",
  "action": false, "outgoing": false
}
```

| Evento | Cuándo | Campos propios |
| --- | --- | --- |
| `load` | El plugin arranca en la red (al activarlo o al iniciar xBNC). | — |
| `connected` | xBNC termina de conectar a la red. | — |
| `message` | Un mensaje o aviso en un canal o privado, también los tuyos. | command, conversation, channel, from, from\_mask, text, action, outgoing |
| `command` | Alguien escribe /msg \*id texto. | text |
| `timer` | Cada minuto, o cada timer\_secs (permiso timer). | — |
| `join`, `part`, `kick`, `quit`, `nick`, `names` | Movimientos en los canales (permiso presence). | los datos de cada movimiento |
| `http` | La respuesta a una petición web (permiso http). | id, status, body, error |

Campos comunes: `network`, `nick` (tu nick actual), `connected`, `clients` (clientes conectados), `lang` (tu idioma), `config` (sus ajustes) y `state` (lo que guardó).

### Respuesta

```json
{
  "actions": [
    { "reply": "texto en la misma conversación" },
    { "tell": "texto solo para ti, como *id" },
    { "privmsg": { "target": "#canal", "text": "..." } },
    { "notice": { "target": "nick", "text": "..." } },
    { "raw": "MODE #canal +o bob" },
    { "http": { "id": "tiempo", "method": "GET", "url": "https://api.open-meteo.com/..." } }
  ],
  "state": { "lo que quieras guardar": 1 }
}
```

- Textos de una sola línea. Las acciones sin permiso o mal formadas se descartan y quedan en el registro.
- `state` sustituye el estado guardado del plugin en esa red; llega en cada evento y sobrevive a los reinicios. La instancia se mantiene entre eventos mientras la red sigue activa.
- `raw` nunca puede enviar QUIT, PASS, USER, NICK, CAP, AUTHENTICATE, OPER, SERVER ni SQUIT.
- `http`: `('id', 'url', 'method', 'body')`. Solo https, a dominios aprobados; métodos GET, POST, PUT y DELETE; cuerpo de hasta 16 KB (texto o JSON); hasta 3 peticiones a la vez y 30 por minuto por plugin y red. La respuesta llega como evento `http` con `id`, `status`, `body` (hasta 256 KB) y, si falló, `error`.

### Límites

| Límite | Valor |
| --- | --- |
| Tamaño del fichero | 4 MB |
| Memoria | 16 MB |
| CPU por evento | unos 20 millones de instrucciones (50 millones al arrancar) |
| Respuesta | 64 KB y 20 acciones; 5 mensajes por evento |
| Estado guardado | 64 KB |
| Almacén clave-valor | 16 MB y 200 000 claves por plugin y red |
| Fallos seguidos antes de detenerse en una red | 5 |

### SDK de C

`sdk/c/xbnc_plugin.h` resuelve la interfaz. Funciones principales:

| Función | Qué hace |
| --- | --- |
| `void on_event(const xbnc_event *ev)` | La función que escribes tú: recibe cada evento. |
| `xbnc_get(ev, "campo", buf, max)` | Lee un campo de texto del evento. |
| `xbnc_is(ev, "campo")` | Lee un campo sí/no. |
| `xbnc_config(ev, "clave", buf, max)`, `xbnc_config_int`, `xbnc_config_bool` | Lee un ajuste. |
| `xbnc_get_state(ev)` | El estado guardado (JSON). |
| `xbnc_reply(texto)`, `xbnc_tell(texto)` | Responder en la conversación o solo a ti. |
| `xbnc_privmsg(destino, texto)`, `xbnc_notice(destino, texto)` | Mensaje o aviso a quien quieras. |
| `xbnc_raw(línea)` | Comando IRC (permiso raw). |
| `xbnc_http_get(id, url)`, `xbnc_http_post_json(id, url, json)` | Peticiones web (permiso http). |
| `xbnc_state(json)` | Guardar el estado. |
| `xbnc_kv_get`, `xbnc_kv_set`, `xbnc_kv_del`, `xbnc_kv_list` (y las variantes \_str) | Almacén clave-valor. |
| `xbnc_log(texto)` | Escribir en el registro. |

`sdk/c/xbnc_util.h` añade lo que usan los plugins oficiales: idioma de la respuesta (`u_lang`), lectura de comandos con alias (`u_command`), recorrido del almacén (`u_next_key`), fechas, zonas horarias con horario de verano europeo y construcción de textos.

```c
#include "xbnc_plugin.h"

void on_event(const xbnc_event *ev) {
    static char kind[16], text[256], greeting[128];
    xbnc_get(ev, "event", kind, sizeof kind);
    if (!xbnc_streq(kind, xbnc_strlen(kind), "message")) return;
    xbnc_get(ev, "text", text, sizeof text);
    if (!xbnc_streq(text, xbnc_strlen(text), "!hola")) return;
    xbnc_config(ev, "greeting", greeting, sizeof greeting);
    xbnc_reply(greeting[0] ? greeting : "¡Hola!");
}
```

```sh
cd sdk/c
clang --target=wasm32 -O2 -nostdlib -fno-builtin -I. \
  -Wl,--no-entry -Wl,--export-dynamic -Wl,--strip-debug -Wl,-z,stack-size=65536 \
  -o hola.raw.wasm hola.c
xbncd plugin pack hola.raw.wasm hola.json -o hola.wasm
xbncd plugin check hola.wasm
sudo -u xbnc xbncd plugin install hola.wasm

# Los ejemplos oficiales: make (o make bundle para copiarlos al catálogo)
```

Necesita clang y wasm-ld (LLVM 15 o posterior) y llvm-objcopy (o `xbncd plugin pack`).

### SDK de Rust

El crate `xbnc-plugin` (`sdk/rust`) da el trait `Plugin`, el tipo `Event` (con `kind()`, `text()`, `conversation()`, `is_channel()`, `outgoing()`, `config_str()`, `state()`…), `Response` (con `reply()`, `tell()`, `state()`…) y la macro `plugin!`. Ejemplo incluido (`sdk/rust/examples/stats.rs`):

```rust
//! Example plugin: counts messages per channel and answers "!stats".
//! The counts survive restarts thanks to the persisted state.
//!
//! Build: cargo build -p xbnc-plugin --release --target wasm32-unknown-unknown --example stats
//! then: xbncd plugin pack target/wasm32-unknown-unknown/release/examples/stats.wasm \
//!         sdk/rust/examples/stats.json -o stats.wasm

use xbnc_plugin::{plugin, Event, Plugin, Response};

#[derive(Default)]
struct Stats;

impl Plugin for Stats {
    fn on_event(&mut self, ev: &Event) -> Response {
        let mut r = Response::new();
        let mut counts = ev.state().as_object().cloned().unwrap_or_default();
        match ev.kind() {
            "message" if ev.is_channel() && !ev.outgoing() => {
                let chan = ev.conversation().to_lowercase();
                let n = counts.get(&chan).and_then(|v| v.as_u64()).unwrap_or(0) + 1;
                counts.insert(chan, n.into());
                if ev.text().trim() == ev.config_str("trigger") {
                    r.reply(format!("{} messages counted in {}", n, ev.conversation()));
                }
                r.state(counts.into());
            }
            "command" => {
                if counts.is_empty() {
                    r.tell("Nothing counted yet.");
                }
                let mut v: Vec<_> = counts.iter().collect();
                v.sort_by_key(|e| std::cmp::Reverse(e.1.as_u64()));
                for (chan, n) in v.into_iter().take(10) {
                    r.tell(format!("{chan}: {n}"));
                }
            }
            _ => {}
        }
        r
    }
}

plugin!(Stats);
```

### Modo desarrollo

En *Plugins → Desarrollo* (superadministradores): con el modo activo, xBNC vigila `/var/lib/xbnc/plugins/dev/`. Un `.wasm` guardado ahí se comprueba e instala en cuanto cambia y todas las redes lo recargan. Si pones `nombre.json` junto a `nombre.wasm`, lo empaqueta solo (no hace falta `xbncd plugin pack`). Un plugin nuevo se activa en las redes de quien activó el modo. Los resultados y lo que escribe el plugin se ven en el registro en directo de la página.

```sh
sudo cp hola.wasm hola.json /var/lib/xbnc/plugins/dev/
```

### Publicar un catálogo propio

Además de los oficiales, xBNC puede ofrecer los plugins de un catálogo remoto firmado (`[catalog]` en `xbnc.toml`). El índice es un JSON `{"plugins": [{"id", "version", "url", "sha256", "size", "manifest"}]}` y su firma Ed25519 en base64 se publica en `índice.sig`. Cada plugin se comprueba contra el SHA-256 firmado antes de instalarlo.

```sh
xbncd plugin catalog-keygen -o catalogo.key        # una vez; la pública va en [catalog] public_key
xbncd plugin catalog-entry hola.wasm https://plugins.ejemplo.org/hola.wasm   # entrada para el índice
xbncd plugin catalog-sign --key catalogo.key index.json                     # escribe index.json.sig
```
