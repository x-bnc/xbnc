<a id="commands"></a>

# Comandos IRC

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

Se escriben desde tu cliente con `/msg *servicio comando`. Las respuestas salen en tu idioma.

### \*status

Es el servicio principal: todo lo que se hace en el panel con una red se puede hacer también aquí. `/msg *status help` lista los comandos **agrupados y con el nombre en negrita**, solo los que tu cuenta puede usar; `/msg *status help <comando>` explica uno con un ejemplo, y `/msg *status help <palabra>` busca (por ejemplo `help canal`). Los nombres no distinguen mayúsculas, y se aceptan también los nombres que usan otros bouncers (`ListChans`, `ListMods`, `AddTrustedServerFingerprint`…), indicados en cada comando como «También vale».

```text
<*status> xBNC: comandos de *status
<*status> Se escriben como /msg *status <comando>. Detalles y un ejemplo: help <comando>.
<*status> Canales
<*status>  channels Tus canales: usuarios, modos y estado
<*status>  nicks <#canal>   Quién está en un canal, con su rango
<*status>  topics   El tema de cada canal
```

Si no se indica otra cosa, el comando lo puede usar cualquier cuenta. Los demás indican quién: las cuentas **con permiso de cambios** (todos los roles menos «limitado»), los **administradores** (administrador y superadministrador) o los **superadministradores**. Las acciones de administración quedan en la [auditoría](USERS_AND_SECURITY.md#security).

#### General

| Comando | Qué hace |
| --- | --- |
| `help [comando]` | Esta lista, o los detalles de un comando. Sin comando, lista todos los que puedes usar. Con uno, explica cómo funciona y da un ejemplo. También busca: help canal.<br>Ejemplo: `/msg *status help addserver` |
| `version` | Versión de xBNC.<br>Ejemplo: `/msg *status version` |
| `uptime` | Cuánto llevan en marcha xBNC y esta red.<br>Ejemplo: `/msg *status uptime` |
| `language <código>` | Tu idioma, también para el panel web. Cambia al momento el idioma de los mensajes de xBNC, de los plugins y del panel web. Sin código, lista los idiomas instalados.<br>Ejemplo: `/msg *status language en` |
| `seen` | Cuándo te conectaste por última vez.<br>Ejemplo: `/msg *status seen` |
| `showmotd` | El mensaje del día de este servidor.<br>Ejemplo: `/msg *status showmotd` |
| `clients [usuario]` | Dispositivos conectados a esta red. Los administradores pueden indicar un usuario para ver todos los suyos.<br>Ejemplo: `/msg *status clients`<br>También vale: `listclients` |

#### Redes y conexión

| Comando | Qué hace |
| --- | --- |
| `status` | Estado de esta red: servidor, TLS, nick, capacidades.<br>Ejemplo: `/msg *status status` |
| `networks` | Tus redes y su estado.<br>Ejemplo: `/msg *status networks`<br>También vale: `listnetworks` |
| `addnetwork <nombre> <servidor> [[+]puerto]` | Añadir una red a tu cuenta. Un + delante del puerto significa TLS. Sin puerto: 6697 con TLS. Tu nick será el de esta red. Después conéctate con usuario/nombre.<br>Ejemplo: `/msg *status addnetwork libera irc.libera.chat +6697` |
| `delnetwork <nombre> confirmar` | Borrar una de tus redes y su historial. Pide confirmación: repite el comando añadiendo «confirmar». No se puede deshacer.<br>Ejemplo: `/msg *status delnetwork pruebas confirmar` |
| `setnick <nick>` | Cambiar el nick guardado de esta red.<br>Ejemplo: `/msg *status setnick y2k`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `connect` | Volver a conectar a la red.<br>Ejemplo: `/msg *status connect`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `disconnect` | Desconectar y quedarse desconectado. xBNC se queda desconectado de esta red hasta que uses connect.<br>Ejemplo: `/msg *status disconnect`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `jump [servidor]` | Reconectar, al siguiente servidor o al que digas. Sin servidor, pasa al siguiente de la lista (servers). Con uno, conecta a ese.<br>Ejemplo: `/msg *status jump irc2.undernet.org`<br>Solo cuentas con permiso de cambios (no el rol limitado). |

#### Servidores, certificados y direcciones

| Comando | Qué hace |
| --- | --- |
| `servers` | Servidores de esta red (principal y de reserva). Si un servidor falla, xBNC prueba el siguiente por turnos.<br>Ejemplo: `/msg *status servers`<br>También vale: `listservers` |
| `addserver <servidor> [[+]puerto] [contraseña]` | Añadir un servidor de reserva. Un + delante del puerto significa TLS. Sin puerto: el del servidor principal. Hasta 20.<br>Ejemplo: `/msg *status addserver irc2.undernet.org 6667`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `delserver <servidor> [puerto]` | Quitar un servidor. Si quitas el principal, el primero de reserva pasa a ser el principal.<br>Ejemplo: `/msg *status delserver irc2.undernet.org`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `addfingerprint <SHA-256>` | Confiar en el certificado de un servidor por su huella. Para servidores con certificado propio o caducado. 64 dígitos hexadecimales, con o sin dos puntos. Se aplica en la próxima conexión.<br>Ejemplo: `/msg *status addfingerprint 3F:9A:...:C2:1E`<br>También vale: `addtrustedserverfingerprint`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `delfingerprint <SHA-256>` | Dejar de confiar en un certificado.<br>Ejemplo: `/msg *status delfingerprint 3F:9A:...:C2:1E`<br>También vale: `deltrustedserverfingerprint`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `fingerprints` | Certificados de confianza de esta red.<br>Ejemplo: `/msg *status fingerprints`<br>También vale: `listtrustedserverfingerprints` |
| `showbindhost` | Dirección local con la que se conecta.<br>Ejemplo: `/msg *status showbindhost` |
| `setbindhost <IP>` | Dirección local para esta red (vhost). Una de las direcciones IP de este servidor. Se aplica en la próxima conexión (jump).<br>Ejemplo: `/msg *status setbindhost 203.0.113.25`<br>Solo administradores. |
| `clearbindhost` | Usar la dirección automática en esta red.<br>Ejemplo: `/msg *status clearbindhost`<br>Solo administradores. |
| `setuserbindhost <IP>` | Dirección local por defecto para todas tus redes. La dirección propia de una red (setbindhost) tiene prioridad.<br>Ejemplo: `/msg *status setuserbindhost 2001:db8::25`<br>Solo administradores. |
| `clearuserbindhost` | Quitar tu dirección local por defecto.<br>Ejemplo: `/msg *status clearuserbindhost`<br>Solo administradores. |

#### Canales

| Comando | Qué hace |
| --- | --- |
| `channels` | Tus canales: usuarios, modos y estado.<br>Ejemplo: `/msg *status channels`<br>También vale: `listchans` |
| `nicks <#canal>` | Quién está en un canal, con su rango.<br>Ejemplo: `/msg *status nicks #xbnc`<br>También vale: `listnicks` |
| `topics` | El tema de cada canal.<br>Ejemplo: `/msg *status topics` |
| `enablechan <#canales>` | Entrar en canales guardados que estaban desactivados. Admite varios canales y los comodines \* y ?.<br>Ejemplo: `/msg *status enablechan #xbnc #xbnc-dev`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `joinnow` | Entrar ya en tus canales, sin esperar al host oculto. Cuando pides el host oculto (/mode $nick +x en los comandos al conectar, o el módulo Login en servicios), xBNC no entra en ningún canal hasta que el servidor oculta tu host, así nunca se ve tu dirección real. Este comando entra en ellos ya de todas formas.<br>Ejemplo: `/msg *status joinnow` |
| `disablechan <#canales>` | Salir de canales pero dejarlos guardados. No se vuelve a entrar en ellos hasta enablechan. Admite \* y ?.<br>Ejemplo: `/msg *status disablechan #xbnc*`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `attach <#canales>` | Volver a mostrar canales desacoplados.<br>Ejemplo: `/msg *status attach #xbnc` |
| `detach <#canales>` | Ocultar canales del cliente (siguen grabando). Una mención trae el canal de vuelta. Admite \* y ?.<br>Ejemplo: `/msg *status detach #xbnc`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `movechan <#canal> <posición>` | Cambiar la posición de un canal en el orden. El orden es en el que xBNC entra en tus canales y los lista.<br>Ejemplo: `/msg *status movechan #xbnc 1`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `swapchans <#canal1> <#canal2>` | Intercambiar la posición de dos canales.<br>Ejemplo: `/msg *status swapchans #xbnc #xbnc-dev`<br>Solo cuentas con permiso de cambios (no el rol limitado). |

#### Historial (buffers)

| Comando | Qué hace |
| --- | --- |
| `playbuffer <#canal\|nick> [líneas]` | Reproducir el historial de una conversación. Te envía las últimas líneas (50 por defecto, hasta 1000).<br>Ejemplo: `/msg *status playbuffer #xbnc 100` |
| `clearbuffer <#canal\|nick>` | Borrar el historial de una conversación. Admite \* y ?.<br>Ejemplo: `/msg *status clearbuffer bob`<br>También vale: `clearhistory` |
| `clearallbuffers` | Borrar todo el historial de esta red.<br>Ejemplo: `/msg *status clearallbuffers` |
| `clearallchannelbuffers` | Borrar el historial de todos los canales.<br>Ejemplo: `/msg *status clearallchannelbuffers` |
| `clearallquerybuffers` | Borrar el historial de todos los privados.<br>Ejemplo: `/msg *status clearallquerybuffers` |
| `setbuffer <#canal> <días\|siempre\|nada\|servidor>` | Cuánto tiempo se guarda el historial de un canal. siempre: no se borra; nada: no se guarda; servidor: lo general; o un número de días.<br>Ejemplo: `/msg *status setbuffer #xbnc 90`<br>Solo cuentas con permiso de cambios (no el rol limitado). |

#### Módulos y plugins

| Comando | Qué hace |
| --- | --- |
| `modules` | Módulos y plugins de esta red y su estado.<br>Ejemplo: `/msg *status modules`<br>También vale: `listmods` |
| `availmods` | Todos los módulos y plugins, con lo que hacen.<br>Ejemplo: `/msg *status availmods`<br>También vale: `listavailmods` |
| `enable <módulo\|plugin>` | Activar un módulo o plugin en esta red.<br>Ejemplo: `/msg *status enable away`<br>También vale: `loadmod`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `disable <módulo\|plugin>` | Desactivarlo.<br>Ejemplo: `/msg *status disable away`<br>También vale: `unloadmod`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `reloadmod <módulo\|plugin>` | Reiniciar un módulo o plugin en esta red.<br>Ejemplo: `/msg *status reloadmod trivia`<br>También vale: `updatemod`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `get <módulo\|plugin>` | Ver sus ajustes.<br>Ejemplo: `/msg *status get away` |
| `set <módulo> <ajuste> <valor>` | Cambiar un ajuste. Para ajustes sí/no: on/off. Mira los nombres con get.<br>Ejemplo: `/msg *status set away delay 60`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `addline <módulo> <ajuste> <texto>` | Añadir una línea a un ajuste de lista.<br>Ejemplo: `/msg *status addline watch patterns *xbnc*`<br>Solo cuentas con permiso de cambios (no el rol limitado). |
| `dellines <módulo> <ajuste>` | Vaciar un ajuste de lista.<br>Ejemplo: `/msg *status dellines watch patterns`<br>Solo cuentas con permiso de cambios (no el rol limitado). |

#### Administración

| Comando | Qué hace |
| --- | --- |
| `users` | Todas las cuentas y si están conectadas.<br>Ejemplo: `/msg *status users`<br>También vale: `listusers`<br>Solo administradores. |
| `allnetworks` | Todas las redes de todos los usuarios, con su estado.<br>Ejemplo: `/msg *status allnetworks`<br>También vale: `listallusernetworks`<br>Solo administradores. |
| `traffic` | Tráfico y mensajes desde que arrancó xBNC.<br>Ejemplo: `/msg *status traffic`<br>Solo administradores. |
| `broadcast <mensaje>` | Enviar un aviso a todos los usuarios conectados.<br>Ejemplo: `/msg *status broadcast Reinicio a las 23:00`<br>Solo administradores. |
| `movenetwork <usuario> <red> <usuario nuevo> [nombre nuevo]` | Pasar una red a otro usuario. La red se reconecta como el nuevo usuario, con su historial.<br>Ejemplo: `/msg *status movenetwork ana libera bob`<br>Solo administradores. |
| `rehash` | Releer usuarios, redes, canales e idiomas. Para cambios hechos con la terminal (xbncd). Los cambios en xbnc.toml necesitan reiniciar.<br>Ejemplo: `/msg *status rehash`<br>También vale: `reload`<br>Solo administradores. |
| `listports` | Puertos en los que escucha xBNC. Los puertos se configuran en xbnc.toml (o con el instalador) y necesitan reiniciar.<br>Ejemplo: `/msg *status listports`<br>Solo administradores. |
| `saveconfig` | Guardar la configuración. No hace falta: xBNC guarda cada cambio al momento.<br>Ejemplo: `/msg *status saveconfig`<br>Solo administradores. |
| `setmotd <texto>` | Poner el mensaje del día de este servidor. Los usuarios lo reciben de \*status al conectar. Hasta 20 líneas.<br>Ejemplo: `/msg *status setmotd Bienvenidos a xBNC`<br>Solo superadministradores. |
| `addmotd <texto>` | Añadir una línea al mensaje del día.<br>Ejemplo: `/msg *status addmotd Normas: /msg *status help`<br>Solo superadministradores. |
| `clearmotd` | Quitar el mensaje del día.<br>Ejemplo: `/msg *status clearmotd`<br>Solo superadministradores. |
| `restart [mensaje]` | Reiniciar xBNC. Todas las redes se reconectan. El mensaje, si lo hay, se envía antes a todos.<br>Ejemplo: `/msg *status restart Actualizando`<br>Solo superadministradores. |
| `shutdown [mensaje]` | Parar xBNC por completo. Queda parado hasta que se arranque de nuevo en el servidor (systemctl start xbnc).<br>Ejemplo: `/msg *status shutdown Mantenimiento`<br>Solo superadministradores. |

#### Notas sobre estos comandos

- **Servidores de reserva.** Si la conexión a un servidor falla, xBNC prueba el siguiente de la lista al segundo, y tras recorrerla entera vuelve a esperar como siempre (5 s, 10 s, 20 s… hasta 5 min). Si un servidor ya conectado corta la conexión, xBNC vuelve primero a ese mismo servidor. Las contraseñas de los servidores de reserva se guardan cifradas. `jump` sin nada pasa al siguiente; `jump servidor` o `jump servidor:puerto`, a uno concreto.
- **Certificados de confianza.** Para redes con certificado propio, caducado o con otro nombre. xBNC sigue comprobando todo lo demás de TLS y solo acepta ese certificado concreto (su huella SHA-256). Para ver la huella de un servidor: `openssl s_client -connect irc.ejemplo.org:6697 </dev/null 2>/dev/null | openssl x509 -noout -fingerprint -sha256`
- **Dirección local (vhost).** Solo administradores, porque usa las direcciones IP del servidor. xBNC comprueba que la dirección pertenece a la máquina antes de guardarla. La de la red tiene prioridad sobre la del usuario. Se aplica en la siguiente conexión.
- **Canales desactivados.** Siguen guardados con su clave y su configuración, pero xBNC no entra en ellos al conectar. El panel los marca como «Desactivado». `enablechan` entra en ellos al momento.
- **Orden de los canales.** Es el orden en el que xBNC entra al conectar y en el que los lista `channels` y el panel. Los canales nuevos van al final.
- **Comodines.** `enablechan`, `disablechan`, `attach`, `detach` y `clearbuffer` aceptan varios nombres separados por espacios o comas, y los comodines `*` y `?`: `disablechan #xbnc* #xbnc-dev`.
- **Mensaje del día.** Lo pone un superadministrador y lo recibe cada usuario, como aviso de `*status`, cada vez que conecta un cliente. Hasta 20 líneas de 400 caracteres. Se ve con `showmotd`.
- **Avisos a todos.** `broadcast` envía «\[Aviso del administrador\] texto» a cada red en marcha; lo reciben todos los clientes conectados. `restart` y `shutdown` con mensaje lo envían antes.
- **Reiniciar y parar.** `restart` reinicia xBNC dentro del mismo proceso (las redes se reconectan). `shutdown` lo para del todo y systemd no lo vuelve a arrancar: hay que hacerlo con `sudo systemctl start xbnc`.
- **Puertos y xbnc.toml.** Los puertos y el resto de [xbnc.toml](CONFIGURATION.md#configuration) no se cambian desde IRC: `listports` los muestra y `rehash` relee usuarios, redes, canales e idiomas (lo mismo que `sudo systemctl reload xbnc`). `saveconfig` existe por costumbre: xBNC guarda cada cambio al momento.

### \*notes

| Comando | Qué hace |
| --- | --- |
| `list` | Tus notas. |
| `get <clave>` | Ver una nota. |
| `add <clave> <texto>` | Guardar una nota (hasta 400 caracteres). |
| `del <clave>` | Borrar una nota. |

Son las mismas notas que en *Tu cuenta → Notas*.

### \*watch

Aquí llegan las coincidencias del módulo [Vigilar palabras](MODULES.md#mod-watch). `/msg *watch help` recuerda cómo configurarlo.

### \*crypt

| Comando | Qué hace |
| --- | --- |
| `setkey <#canal\|nick> [cbc:]<clave>` | Guardar la clave FiSH de un canal o privado (cbc: para el modo CBC). Se guarda cifrada. |
| `delkey <#canal\|nick>` | Borrar una clave. |
| `keyx <nick>` | Acordar una clave en privado con DH1080 (la otra persona necesita FiSH). |
| `list` | Conversaciones con clave. |

Escribe `+p` delante de un mensaje para enviarlo sin cifrar. Ver [Cifrado de mensajes](MODULES.md#mod-crypt).

### \*ignore

| Comando | Qué hace |
| --- | --- |
| `add <nick o máscara>` | Ignorar a alguien (nick, o nick!usuario@host con \*). |
| `del <nick o máscara>` | Dejar de ignorar. |
| `list` | Lista de ignorados. |

### Un servicio por plugin

Cada plugin activo responde en `*id` (`*trivia`, `*remind`, `*poll`…). Con `ayuda` o cualquier texto que no entienda, explica cómo se usa. Si un plugin no tiene nada que decir, xBNC responde con su nombre, versión, descripción y uso.

### Comandos de cliente

| Comando | Qué hace |
| --- | --- |
| `/quote DETACH #canal` | Desacopla un canal: sigue guardando, pero no aparece en tu cliente. |
| `/quote ATTACH #canal` o `/join #canal` | Vuelve a mostrar un canal desacoplado, con los últimos mensajes. |
| `/quote <alias> [argumentos]` | Ejecuta un [alias](MODULES.md#mod-alias) si tu cliente no envía comandos que no conoce. |

> Escribe siempre algo después del nombre: `/msg *poll ayuda`. Algunos clientes, como HexChat, muestran su propia ayuda de /msg («Uso: MSG \<apodo\> \<mensaje\>») si no escribes nada detrás.
