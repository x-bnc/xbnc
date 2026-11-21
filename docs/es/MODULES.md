<a id="modules"></a>

# Módulos

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

Los módulos son funciones nativas de xBNC, compiladas dentro del programa, que se activan y configuran **por red**: en el panel (*Módulos*), con la API o desde IRC con `/msg *status`. Los cambios se aplican al momento, sin reconectar. Toda la configuración se valida (tipos, rangos, longitudes, sin caracteres de control). Las contraseñas y claves se guardan cifradas y el panel no las vuelve a mostrar: para cambiarlas se escriben de nuevo.

```
/msg *status modules                         lista y estado
/msg *status enable keepnick                 activar (disable para desactivar)
/msg *status get away                        ver ajustes
/msg *status set away delay 60               cambiar un ajuste
/msg *status addline watch patterns *xbnc*   añadir una línea a una lista
/msg *status dellines watch patterns         vaciar una lista
/msg *status availmods                       todos los módulos y plugins, con lo que hacen
```

Tipos de ajuste: **sí/no** (en `*status`, `true`, `on` o `1` para sí y `false`, `off` o `0` para no), **número** (con mínimo y máximo), **texto**, **lista** (una entrada por línea), **opción** (uno de los valores indicados) y **secreto** (se guarda cifrado).

### Conexión y cuenta

<a id="mod-services"></a>

#### Login en servicios

`services` · *nuevo en 1.0*

Te identifica con X (Undernet), NickServ, Q o AuthServ al conectar y oculta tu IP antes de entrar en los canales.

En Undernet usa Login on Connect: la cuenta va en la contraseña del servidor y tu IP real no llega a verse. Si la red ya tiene contraseña de servidor, se identifica con /msg X login después de conectar. Los canales esperan a que la red confirme el inicio de sesión. Con «Ocultar mi IP», nunca se entra en ningún canal antes de que la red oculte tu IP, aunque tarde más que la espera (/msg \*status joinnow entra igualmente). La contraseña se guarda cifrada y no se vuelve a mostrar.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Servicio (`service`) | opción | auto | Opciones: `auto` Automático según la red, `x` X (Undernet), `nickserv` NickServ (Libera, Rizon, EvilNet…), `q` Q (QuakeNet), `authserv` AuthServ (GameSurge, AfterNet). |
| Cuenta (`account`) | texto | vacío | Tu nombre de cuenta en el servicio. Con NickServ puede quedar vacío si es tu nick. |
| Contraseña (`password`) | secreto | vacío |   |
| Ocultar mi IP (modo +x) (`hide_host`) | sí/no | sí | En X, Q y AuthServ. En el resto de redes lo hace la propia red. |
| Identificarse al conectar (Login on Connect) (`login_on_connect`) | sí/no | sí | Solo Undernet: la IP real no llega a verse nunca. |
| Esperar antes de entrar en los canales (segundos) (`wait`) | número (0–120) | 15 | Hasta que la red confirme el inicio de sesión. 0: no esperar. Con «Ocultar mi IP», los canales siempre esperan a que la IP esté oculta. |
| Recuperar mi nick si está ocupado (`recover_nick`) | sí/no | sí | Con NickServ: pide el nick (REGAIN o GHOST) al identificarte. |

**Ejemplo**

```text
Conectando a irc.ejemplo.org:6667 con Login on Connect…
*** Tu host es ahora y2k.users.ejemplo.org
* y2k ha entrado en #xbnc
```

Desde IRC: `/msg *status enable services` y `/msg *status get services`

<a id="mod-invite"></a>

#### Invitación y desbaneo

`invite` · *nuevo en 1.0*

Si no puedes entrar en un canal (+i, +k, +l o ban), pide a X o ChanServ que te invite o te quite el ban y vuelve a entrar.

Lo intenta como mucho tres veces cada diez minutos por canal. Necesitas acceso en el canal para que el servicio te haga caso.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Servicio de canales (`service`) | opción | auto | Opciones: `auto` Automático según la red, `x` X (Undernet), `chanserv` ChanServ, `q` Q (QuakeNet). |
| Pedir que me quiten el ban (`unban`) | sí/no | sí |   |
| Entrar cuando me inviten a uno de mis canales (`join_invites`) | sí/no | sí |   |

**Ejemplo**

```text
#xbnc: no puedes entrar (+i)
<*status> Pidiendo a X que te deje entrar en #xbnc (+i).
* y2k ha entrado en #xbnc
```

Desde IRC: `/msg *status enable invite` y `/msg *status get invite`

<a id="mod-cert"></a>

#### Certificado de cliente (CertFP)

`cert`

Te identifica en la red con un certificado en lugar de una contraseña. Permite SASL EXTERNAL.

Genera un certificado, añade su huella a tu cuenta de la red y reconecta. En Libera.Chat: conéctate identificado y escribe /msg NickServ CERT ADD. Solo funciona con TLS.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Usar SASL EXTERNAL si la red lo ofrece (`sasl_external`) | sí/no | sí |   |

**Ejemplo**

```text
<y2k> /msg NickServ CERT ADD
<NickServ> Added fingerprint 3f9a…c21e to your fingerprint list.
```

Desde IRC: `/msg *status enable cert` y `/msg *status get cert`

<a id="mod-keepnick"></a>

#### Recuperar nick

`keepnick` · *activo por defecto*

Si tu nick está ocupado al conectar, xBNC lo recupera en cuanto quede libre.

Comprueba cada 30 segundos si tu nick está libre y lo recupera al momento cuando quien lo tiene se desconecta. Si cambias de nick a mano desde tu cliente, se pausa hasta la próxima conexión.

**Ejemplo**

```text
<*status> Tu nick y2k está ocupado; conectado como y2k_. Lo recupero en cuanto quede libre.
* y2k_ ahora se llama y2k
```

Desde IRC: `/msg *status enable keepnick` y `/msg *status get keepnick`

### Canales

<a id="mod-stickychan"></a>

#### Canales fijos

`stickychan`

Canales de los que no puedes salir por error: xBNC bloquea el /part y vuelve a entrar si sales.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Canales (`channels`) | lista | vacío | Uno por línea. |

**Ejemplo**

```
#xbnc
#xbnc-dev

Si escribes /part #xbnc, xBNC no sale del canal.
```

Desde IRC: `/msg *status enable stickychan` y `/msg *status get stickychan`

<a id="mod-kickrejoin"></a>

#### Volver tras un kick

`kickrejoin`

Vuelve a entrar en el canal si te echan. Deja de intentarlo si te echan más de 3 veces en un minuto.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Espera antes de volver a entrar (segundos) (`delay`) | número (0–600) | 10 |   |

**Ejemplo**

```text
* bob te ha echado de #xbnc (prueba)
* y2k ha entrado en #xbnc
```

Desde IRC: `/msg *status enable kickrejoin` y `/msg *status get kickrejoin`

<a id="mod-autoop"></a>

#### Op y voz automáticos

`autoop`

Da op, halfop o voz a las personas que elijas cuando entran en tus canales (si tú tienes op).

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Espera antes de dar el modo (segundos) (`delay`) | número (0–30) | 3 |   |
| Personas (`entries`) | lista | vacío | Una por línea: modo (o, h o v), máscara y, si quieres, canales separados por comas (o \*) y una clave. Con clave, xBNC envía un desafío y solo da el modo si la otra persona responde con la misma clave desde su propio xBNC. |

**Ejemplo**

```
o *!*@casa.ejemplo.org #xbnc
v amigo!*@* *
o bob!*@* #xbnc clave-compartida
```

Desde IRC: `/msg *status enable autoop` y `/msg *status get autoop`

<a id="mod-autocycle"></a>

#### Recuperar op al quedarte solo

`autocycle` · *nuevo en 1.0*

Si te quedas solo en un canal sin op, sales y vuelves a entrar para conseguirlo. Nunca en canales +i o con clave.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Solo en estos canales (`channels`) | lista | vacío | Uno por línea. Vacío: en todos. |

**Ejemplo**

```text
* bob ha salido de #xbnc
<*status> Te has quedado solo en #xbnc sin op: salgo y vuelvo a entrar.
* y2k ha entrado en #xbnc (con op)
```

Desde IRC: `/msg *status enable autocycle` y `/msg *status get autocycle`

<a id="mod-detach"></a>

#### Canales desacoplados

`detach` · *nuevo en 1.0*

Canales que siguen guardando todo pero no aparecen en tu cliente. Vuelven a aparecer si te mencionan.

Desde tu cliente: /quote DETACH #canal para desacoplar y /join #canal para volver a verlo. Al volver se muestran los últimos mensajes.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Canales (`channels`) | lista | vacío | Uno por línea. Admite \* (por ejemplo #\*-bots). |
| Mostrar el canal si mencionan mi nick (`on_mention`) | sí/no | sí |   |
| Mostrar el canal si escribe alguno de estos nicks (`nicks`) | lista | vacío | Uno por línea. |

**Ejemplo**

```
/quote DETACH #xbnc     desacopla el canal (sigue guardando)
/quote ATTACH #xbnc     o /join #xbnc: vuelve a mostrarlo

Si alguien escribe tu nick en #xbnc, el canal reaparece con los últimos mensajes.
```

Desde IRC: `/msg *status enable detach` y `/msg *status get detach`

<a id="mod-flooddetach"></a>

#### Ocultar canales inundados

`flooddetach` · *nuevo en 1.0*

Si un canal se llena de mensajes de golpe, lo oculta un rato para que tu cliente no se congele. Se sigue guardando.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Mensajes (`messages`) | número (3–1000) | 25 | Cuántos mensajes… |
| …en cuántos segundos (`seconds`) | número (1–600) | 10 |   |
| Segundos de calma antes de volver a mostrarlo (`calm`) | número (10–3600) | 60 |   |

**Ejemplo**

```
Mensajes: 25
…en cuántos segundos: 10
Segundos de calma antes de volver a mostrarlo: 60
```

Desde IRC: `/msg *status enable flooddetach` y `/msg *status get flooddetach`

### Mensajes

<a id="mod-away"></a>

#### Ausente automático

`away` · *activo por defecto*

Te marca como ausente cuando cierras todos tus clientes y, al volver, te resume lo que te perdiste.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Mensaje de ausencia (`message`) | texto | vacío | Vacío: «Ausente» en tu idioma. |
| Esperar antes de marcarte ausente (segundos) (`delay`) | número (0–3600) | 60 |   |
| Al volver, avisarme de los privados y menciones recibidos (`summary`) | sí/no | sí |   |
| Nick mientras estás ausente (`nick`) | texto | vacío | Vacío: no lo cambia. {nick} es tu nick, por ejemplo {nick}\|away. Se recorta a lo que permita la red (12 caracteres en Undernet). |

**Ejemplo**

```
Mensaje de ausencia: Vuelvo enseguida
Nick mientras estás ausente: {nick}|away

Al volver, *status te resume: «Mientras no estabas: 3 privados (bob, carol) y 2 menciones en #xbnc».
```

Desde IRC: `/msg *status enable away` y `/msg *status get away`

<a id="mod-watch"></a>

#### Vigilar palabras

`watch` · *activo por defecto*

Copia a la conversación \*watch los mensajes que contienen lo que te interesa, aunque no estuvieras conectado.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Avisarme también cuando mencionen mi nick (`mentions`) | sí/no | no |   |
| Patrones (`patterns`) | lista | vacío | Uno por línea. \* vale por cualquier texto: \*despliegue\* encuentra «el despliegue falló». Después puedes poner un canal o un nick para limitarlo. |

**Ejemplo**

```
*despliegue*
*xbnc* #xbnc
*urgente* bob

Las coincidencias llegan a la conversación *watch.
```

Desde IRC: `/msg *status enable watch` y `/msg *status get watch`

<a id="mod-alias"></a>

#### Alias

`alias` · *activo por defecto*

Tus propios comandos: escribes /ghost bob y xBNC envía /msg NickServ GHOST bob.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Alias (`aliases`) | lista | vacío | Uno por línea: nombre y comando. $1, $2… son los argumentos, $1- todos desde el primero y $nick tu nick. |

**Ejemplo**

```
ghost /msg NickServ GHOST $1
op /mode $1 +o $nick
saluda /msg $1 hola, $1, ¿qué tal?

En tu cliente: /ghost y2k   (o /quote ghost y2k si tu cliente no lo envía)
```

Desde IRC: `/msg *status enable alias` y `/msg *status get alias`

<a id="mod-ignore"></a>

#### Ignorar

`ignore` · *nuevo en 1.0*

No recibes los mensajes de quien ignoras en ningún cliente ni en el historial. En Undernet usa SILENCE para que ni siquiera lleguen.

En IRC: /msg \*ignore add nick, /msg \*ignore del nick y /msg \*ignore list.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Ignorar a (`masks`) | lista | vacío | Uno por línea: un nick o una máscara nick!usuario@host (admite \*). |
| Usar SILENCE si la red lo permite (`silence`) | sí/no | sí |   |

**Ejemplo**

```
/msg *ignore add pesado
/msg *ignore add *!*@spam.example
/msg *ignore list
/msg *ignore del pesado
```

Desde IRC: `/msg *status enable ignore` y `/msg *status get ignore`

<a id="mod-stripcontrols"></a>

#### Quitar colores

`stripcontrols` · *nuevo en 1.0*

Quita colores, negritas, subrayados y parpadeos de los mensajes, también en el historial.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Solo en estos canales (`channels`) | lista | vacío | Uno por línea. Vacío: en todos. |

**Ejemplo**

```
Solo en estos canales:
#xbnc

Antes: \x0304,01rojo\x03 \x02negrita\x02   Después: rojo negrita
```

Desde IRC: `/msg *status enable stripcontrols` y `/msg *status get stripcontrols`

<a id="mod-charset"></a>

#### Codificación de caracteres

`charset` · *nuevo en 1.0*

Las tildes y la ñ se ven bien aunque el canal use Latin-1, y puedes escribir en Latin-1 donde haga falta.

xBNC siempre entiende los mensajes en Latin-1 que no son UTF-8. Este módulo elige la tabla exacta y en qué canales o privados envías tus mensajes en esa codificación (para gente con clientes muy antiguos).

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Codificación antigua (`legacy`) | opción | windows-1252 | Opciones: `windows-1252` Windows-1252 (la más común), `iso-8859-15` ISO-8859-15 (con €), `iso-8859-1` ISO-8859-1 (Latin-1). |
| Enviar en esa codificación a (`send_legacy`) | lista | vacío | Canales o nicks, uno por línea. \* para todos. Vacío: siempre UTF-8. |

**Ejemplo**

```
Codificación antigua: Windows-1252
Enviar en esa codificación a:
#xbnc
abuelo
```

Desde IRC: `/msg *status enable charset` y `/msg *status get charset`

<a id="mod-crypt"></a>

#### Cifrado de mensajes (FiSH)

`crypt` · *nuevo en 1.0*

Cifra tus mensajes en canales y privados con claves compartidas, compatible con FiSH de mIRC, HexChat e irssi.

En IRC: /msg \*crypt setkey #canal clave, /msg \*crypt keyx nick para acordar una clave en privado y +p delante de un mensaje para enviarlo sin cifrar. Las claves se guardan cifradas. FiSH protege de los servidores de la red, no sustituye al cifrado de extremo a extremo.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Claves (`keys`) | lista secreta | vacío | Una por línea: canal o nick y clave (cbc: delante para el modo CBC). Se guardan cifradas: escribe todas otra vez para cambiarlas. |
| Marca para los mensajes descifrados (`prefix`) | texto | vacío | Por ejemplo \[cifrado\] . Vacío: sin marca. |
| Aceptar intercambios de claves (DH1080) (`key_exchange`) | sí/no | sí | Cuando alguien te propone una clave en privado. |

**Ejemplo**

```
/msg *crypt setkey #xbnc cbc:una-clave-larga
/msg *crypt keyx bob          acordar una clave en privado (DH1080)
/msg *crypt list
/msg *crypt delkey #xbnc
+p hola                       envía este mensaje sin cifrar
```

Desde IRC: `/msg *status enable crypt` y `/msg *status get crypt`

<a id="mod-schedule"></a>

#### Tareas programadas

`schedule` · *nuevo en 1.0*

Comandos a una hora fija o cada cierto tiempo, en tu zona horaria.

La zona horaria se elige en Tu cuenta. Las tareas solo se ejecutan mientras la red está conectada.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Tareas (`tasks`) | lista | vacío | Una por línea: hora y comando (08:00 /away Trabajando), días y hora (lun-vie 18:00 /msg #canal me voy) o intervalo (cada 30m /msg #canal sigo aquí). Días: lun mar mie jue vie sab dom, laborables, finde. |

**Ejemplo**

```
lun-vie 08:00 /away Trabajando
lun-vie 15:00 /away
finde 10:30 /msg #xbnc ¡Buenos días!
cada 2h /msg #xbnc-dev sigo aquí
```

Desde IRC: `/msg *status enable schedule` y `/msg *status get schedule`

<a id="mod-ctcp"></a>

#### Respuestas CTCP

`ctcp` · *activo por defecto*

Responde siempre a VERSION (por defecto con la versión de xBNC y su web), responde a PING y TIME cuando no hay clientes conectados y frena las inundaciones de CTCP.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Peticiones CTCP permitidas cada 10 segundos (`flood_limit`) | número (1–100) | 5 |   |
| Respuesta a VERSION (`version`) | texto | xBNC v{version} - https://xbnc.org | {version} se sustituye por la versión de xBNC. Vacío: el texto por defecto. |
| CTCP que ignorar (`block`) | lista | vacío | Uno por línea, por ejemplo TIME o FINGER. |

**Ejemplo**

```
Peticiones CTCP permitidas cada 10 segundos: 5
Respuesta a VERSION: mIRC v7.77
CTCP que ignorar:
FINGER
TIME
```

Desde IRC: `/msg *status enable ctcp` y `/msg *status get ctcp`

<a id="mod-block_motd"></a>

#### Ocultar el MOTD

`block_motd`

No muestra a tus clientes el mensaje del día del servidor.

**Ejemplo**

```
Sin ajustes. Activado, tus clientes no reciben el mensaje del día del servidor.
```

Desde IRC: `/msg *status enable block_motd` y `/msg *status get block_motd`

### Historial y DCC

<a id="mod-events"></a>

#### Eventos en el historial

`events` · *activo por defecto*

Guarda entradas, salidas, kicks, cambios de nick, temas y modos junto a los mensajes.

Los clientes modernos los reciben como eventos reales; el resto, como mensajes de \*events al reproducir el historial.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Guardar entradas, salidas y desconexiones (`joins`) | sí/no | sí | En canales grandes generan mucho ruido. |
| Guardar kicks, cambios de nick, temas y modos (`other`) | sí/no | sí |   |

**Ejemplo**

```
Con «Guardar kicks, cambios de nick, temas y modos» activado, al reproducir el historial
verás también: «bob ha cambiado el tema a: Reunión a las 20:00».
```

Desde IRC: `/msg *status enable events` y `/msg *status get events`

<a id="mod-log"></a>

#### Registros en ficheros

`log`

Escribe cada conversación en ficheros de texto diarios, con el formato clásico de los registros de IRC.

Los ficheros quedan en el servidor, en la carpeta logs junto a la base de datos (por ejemplo /var/lib/xbnc/logs/usuario/red/#canal/2026-11-08.log). Las horas están en tu zona horaria (Tu cuenta).

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Borrar ficheros con más de (días) (`retention_days`) | número (0–3650) | 0 | 0 los conserva para siempre. |
| Incluir eventos (entradas, salidas, kicks…) (`events`) | sí/no | sí |   |

**Ejemplo**

```
/var/lib/xbnc/logs/y2k/ejemplo/#xbnc/2026-11-11.log

[18:30:12] <bob> hola a todos
[18:30:20] * carol ha entrado
```

Desde IRC: `/msg *status enable log` y `/msg *status get log`

<a id="mod-route_replies"></a>

#### Respuestas a quien pregunta

`route_replies` · *activo por defecto*

Las respuestas a WHOIS, WHO, LIST, NAMES o listas de bans llegan solo al dispositivo que las pidió, no a todos.

**Ejemplo**

```
Si haces /whois bob desde el móvil, la respuesta llega solo al móvil,
no al portátil que también está conectado.
```

Desde IRC: `/msg *status enable route_replies` y `/msg *status get route_replies`

<a id="mod-dcc"></a>

#### DCC a través de xBNC

`dcc`

Los envíos de ficheros y chats DCC pasan por xBNC: la otra persona no ve tu IP y funcionan aunque estés detrás de un router.

xBNC abre un puerto para cada envío y une las dos conexiones; no guarda nada en disco. En un servidor con IP pública funciona sin configurar nada; detrás de NAT, indica la IP pública en \[dcc\] public\_ip de xbnc.toml y abre los puertos de \[dcc\] ports en el cortafuegos.

| Ajuste | Tipo | Por defecto | Detalle |
| --- | --- | --- | --- |
| Tamaño máximo por envío (MB) (`max_size_mb`) | número (0–1000000) | 4096 | 0 = sin límite. |
| Segundos para aceptar un envío (`wait`) | número (30–900) | 180 |   |
| Permitir envíos desde direcciones privadas (`private_addresses`) | sí/no | no | Desactivado, xBNC no se conecta a redes internas (10.x, 192.168.x…) aunque alguien en IRC lo pida. |

**Ejemplo**

```
# xbnc.toml, solo si el servidor está detrás de un router (NAT):
[dcc]
public_ip = "203.0.113.10"
ports = "50000-50100"
```

Desde IRC: `/msg *status enable dcc` y `/msg *status get dcc`

### Siempre activos

Funciones del núcleo que no hace falta activar.

| Función | Dónde se configura |
| --- | --- |
| Comandos al conectar | En la configuración de cada red. |
| Guardar canales | Siempre activo: tus /join y /part se recuerdan. |
| SASL y NickServ | En la configuración de cada red. |
| Bloqueo de fuerza bruta | En Ajustes y Seguridad. |
| Notas | En Tu cuenta o con /msg \*notes. |
| Última conexión | En Usuarios y con /msg \*status seen. |
| Historial por dispositivo | Siempre activo, con búsqueda en cada red. |
| Administración web e IRC | Este panel y /msg \*status. |
