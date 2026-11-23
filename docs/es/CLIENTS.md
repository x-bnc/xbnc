<a id="clients"></a>

# Conectar tu cliente IRC

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

Sirve cualquier cliente que permita indicar servidor, puerto y contraseña de servidor. No hace falta ningún plugin.

| Qué quieres | Contraseña de servidor |
| --- | --- |
| Usuario con una sola red | `alicia:contraseña` |
| Elegir red | `alicia/ejemplo:contraseña` |
| Elegir red e identificar el dispositivo | `alicia/ejemplo@portatil:contraseña` |
| El mismo login en otro orden | `alicia@portatil/ejemplo:contraseña` |

- **Servidor:** tu dominio o IP (`sudo xbncd info` lo muestra).
- **Puerto:** el puerto IRC con SSL, desde cualquier sitio: el 8443 si al instalar elegiste un solo puerto para el panel e IRC, el 6697 si elegiste uno para cada cosa. El 6667 sin SSL, solo desde tu red local. Los ejemplos de esta página usan el 6697: pon el tuyo.
- **Contraseña:** la misma con la que entras al panel. También puedes poner `alicia/ejemplo@portatil` en el campo de usuario (USER) y la contraseña sola en PASS, o entrar con SASL PLAIN desde el cliente.
- **Una conexión equivale a una red:** para dos redes, crea dos servidores en tu cliente (`alicia/ejemplo` y `alicia/otra`). Los clientes con `soju.im/bouncer-networks` (Goguma, gamja, Halloy) ven todas tus redes en una sola conexión.
- **Dispositivo:** la parte `@dispositivo` es opcional pero recomendable. Cada dispositivo (`@portatil`, `@movil`, `@trabajo`) recibe su propio historial pendiente.
- Si instalaste por IP, el certificado es propio: activa en tu cliente «aceptar certificados no válidos».

No tienes que averiguar nada de esto: en el panel, *Redes → tu red → Conecta tu cliente IRC* muestra el servidor, el puerto y la contraseña del servidor reales de tu cuenta para cada red, listos para copiar. Escribe ahí el nombre del dispositivo y tu contraseña y forma la línea completa (la contraseña no se envía ni se guarda). `sudo xbncd info` muestra lo mismo en el servidor, con tus usuarios y redes reales.

### Desde internet o desde tu red local

Con un dominio, el servidor no es siempre el mismo:

| Dónde estás | Servidor | Puerto |
| --- | --- | --- |
| En cualquier sitio (internet) | Tu dominio, por ejemplo `bnc.ejemplo.org` | El puerto IRC con SSL (8443 o 6697). El router tiene que redirigirlo a la máquina de xBNC. |
| En casa (la misma red que xBNC) | La IP local de la máquina de xBNC, por ejemplo `192.168.1.50` | 6667 sin SSL. El puerto con SSL también funciona, pero el certificado del dominio no es válido para la IP: activa «aceptar certificados no válidos». |

Desde casa, el dominio solo funciona si tu router lo admite (se llama NAT loopback o hairpin NAT); muchos no lo admiten y la conexión simplemente falla. Por eso el panel tiene las dos opciones, *Desde internet* y *Desde tu red local*, con los datos correctos de cada una.

### HexChat

```
Lista de redes → Añadir: «xBNC Ejemplo»
Editar → Servidor: bnc.ejemplo.org/6697
Marca «Usar SSL para todos los servidores»
Método de inicio de sesión: Contraseña del servidor (/PASS)
Contraseña: alicia/ejemplo@hexchat:tu-contraseña
Con certificado propio: marca «Aceptar certificados SSL no válidos»
```

### mIRC

```
Lo más rápido, en cualquier ventana:
/server bnc.ejemplo.org +6697 alicia/ejemplo@mirc:tu-contraseña

El + delante del puerto activa SSL. Con certificado propio, mIRC pregunta la
primera vez: pulsa Aceptar. Para guardarlo: Archivo → Seleccionar servidor →
Añadir, con los mismos datos.
```

### WeeChat

```
/server add xbnc-ejemplo bnc.ejemplo.org/6697 -tls
/set irc.server.xbnc-ejemplo.password "alicia/ejemplo@weechat:tu-contraseña"
/set irc.server.xbnc-ejemplo.capabilities "server-time,message-tags,batch,draft/chathistory,echo-message"
/connect xbnc-ejemplo

Con certificado propio: /set irc.server.xbnc-ejemplo.tls_verify off
```

### irssi

```
/network add -nick alicia xbnc-ejemplo
/server add -tls -network xbnc-ejemplo -password alicia/ejemplo@irssi:tu-contraseña bnc.ejemplo.org 6697
/connect xbnc-ejemplo

Con certificado propio, añade -notls_verify al /server add
```

### Móvil

**Goguma** (Android) y **Palaver** (iOS) admiten bouncers: añade el servidor `bnc.ejemplo.org`, puerto 6697 con TLS, usuario `alicia` y tu contraseña. Goguma muestra todas tus redes en una sola conexión y pide el historial con `CHATHISTORY`. Para recibir avisos con la aplicación cerrada, usa los [avisos de xBNC](PANEL.md#notifications) (push, Telegram…), o instala el panel como aplicación.

### Otros

KVIrc, Konversation, Textual, Halloy, The Lounge y Quassel (como cliente de una red IRC normal) funcionan con los mismos datos: servidor, puerto con TLS y contraseña de servidor. Los clientes con `draft/chathistory` (Goguma, gamja, Kiwi IRC, Halloy) piden el historial directamente y no reciben reproducción automática.

### Si te equivocas de puerto o de SSL

xBNC detecta los errores habituales y te lo dice en el propio cliente:

| Lo que ves en el cliente | Qué pasa | Qué hacer |
| --- | --- | --- |
| `Closing link: this port uses SSL/TLS. Enable SSL in your IRC client, or connect to port 6667 without SSL.` | Te conectas sin SSL al puerto con SSL. | Activa SSL o usa el 6667 desde casa. |
| `Closing link: this port only accepts connections from the local network...` | Usas el 6667 desde fuera de tu red. | Usa el puerto con SSL (8443 o 6697). |
| Error de certificado o «certificado no válido» | Certificado propio (instalación por IP). | Acepta el certificado o pasa a un dominio con Let's Encrypt. |
| `Usuario o contraseña incorrectos (usuario «alice»)…` | Usuario o contraseña incorrectos. | Usa el mismo usuario y contraseña que en el panel: alice/ejemplo@dispositivo:contraseña. |
| `«TU_CONTRASEÑA» es el ejemplo…` | Se copió el ejemplo sin escribir la contraseña. | Escribe tu contraseña después de los dos puntos. No cuenta como intento fallido. |
| `No hay ninguna red llamada «…» en tu cuenta. Tus redes: …` | La red de la contraseña del servidor no existe (es el nombre corto de la red en el panel). | Usa uno de los nombres de la lista. |
| `Indica qué red usar en la contraseña del servidor…` | Tienes varias redes y no indicaste cuál. | Añade /red al usuario. |
| `Demasiados accesos fallidos desde tu dirección (…): está bloqueada N minutos más` | Tu IP está bloqueada por fallos, en IRC y en el panel. Un cliente que sigue reconectando con una contraseña incorrecta también cuenta. | Corrige la contraseña en el cliente (o ciérralo) y espera, o pide a un administrador que la desbloquee en Seguridad → IPs bloqueadas. |

Cuando el cliente se conecta con SSL a un puerto sin SSL no puede mostrar ningún mensaje: lo indica el registro del servicio (`client sent TLS to a plain-text port`).

### Redes sin SASL (Undernet, QuakeNet)

Algunas redes no usan SASL: te identificas con un bot de servicios, X en Undernet o Q en QuakeNet. xBNC lo hace por ti cada vez que se conecta. Activa el módulo [Login en servicios](MODULES.md#mod-services) (en el asistente, el paso *Cuenta*) con tu cuenta y tu contraseña. Si prefieres hacerlo a mano, escribe los comandos en *Configuración → Comandos al conectar*, uno por línea, como en tu cliente:

```
/mode $nick +x
/msg x@channels.undernet.org login TU_CUENTA TU_CONTRASEÑA
```

Se envían antes de entrar en los canales y se guardan cifrados. Cuando los comandos (o el módulo, con «Ocultar mi IP») piden `+x`, xBNC no entra en ningún canal, y retiene también los JOIN de tu cliente, hasta que la red confirma el host oculto (numérico 396). Tu dirección real nunca se ve en un canal. Si el host no se oculta después de la espera (por ejemplo, porque falló el login con X), los canales siguen esperando y `*status` te dice por qué; se entra en cuanto el host se oculta, o enseguida con `/msg *status joinnow`.

### Compatibilidad de clientes

Los tests automáticos usan un cliente IRC estándar, un cliente con todas las capacidades IRCv3 y un cliente clásico sin ninguna. HexChat, mIRC, WeeChat e irssi se han usado a mano con xBNC; el resto de clientes de la lista siguen los mismos estándares pero no se han comprobado uno a uno. Si algo no funciona con un cliente concreto, avisa con su nombre y versión (ver [Avisar de un problema](TESTING.md#beta)).
