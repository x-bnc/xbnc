<a id="networks"></a>

# Redes

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

### Ajustes de una red

En el panel, *Redes → (tu red) → Configuración*. También con la API (`PATCH /networks/{id}`) o la terminal.

| Ajuste | Qué es |
| --- | --- |
| Nombre corto | El que usas en el login (`alicia/ejemplo`). Cambiarlo no corta la conexión. |
| Servidor y puerto | Dirección de la red. Puerto 6697 con TLS por defecto. |
| TLS | Conexión cifrada con la red. El certificado del servidor se verifica siempre. |
| Nick | Tu nick en la red. Si está ocupado al conectar, xBNC añade «\_» y, con [Recuperar nick](MODULES.md#mod-keepnick), vuelve a él en cuanto queda libre. |
| Usuario (ident) y nombre real | Los campos de USER. Por defecto, el nick. |
| Contraseña de servidor | Se envía con PASS al conectar. Se guarda cifrada y no se vuelve a mostrar. |
| Cuenta y contraseña SASL | Identificación SASL PLAIN. Para SASL EXTERNAL, usa un [certificado de cliente](MODULES.md#mod-cert). |
| Activa | Desactivada, xBNC no se conecta a esa red. |
| Comandos al conectar | Líneas que se envían tras conectar y antes de entrar en los canales (ver abajo). |
| Canales | Canales guardados, con su clave (cifrada). Un /join o /part desde el cliente se guarda solo. |
| Retención por canal | Cuánto se guarda el historial de un canal: lo del servidor, nada, para siempre o N días. |
| Servidores de reserva | Otros servidores de la misma red que xBNC prueba si falla el actual. Con `/msg *status addserver` (ver [Comandos IRC](COMMANDS.md#commands)). |
| Certificados de confianza | Huellas SHA-256 de certificados de servidor que se aceptan aunque no validen (certificado propio o caducado). Con `/msg *status addfingerprint`. |
| Dirección local (vhost) | La IP de este servidor con la que se conecta a la red. Con `/msg *status setbindhost` (solo administradores). |
| Canales desactivados y orden | Un canal desactivado sigue guardado pero no se entra en él; el orden es el de entrada al conectar. Con `/msg *status disablechan` y `movechan`. |

### Comandos al conectar

Se escriben como en tu cliente, uno por línea (hasta 20 y 510 caracteres cada uno), y se guardan cifrados porque suelen llevar contraseñas. `$nick` se cambia por tu nick actual. Se admiten `/msg` (o `/privmsg`, `/query`), `/notice`, `/quote` (o `/raw`) y cualquier comando IRC con barra (`/mode`, `/away`…). Una línea sin barra se envía tal cual.

```
/mode $nick +x
/msg x@channels.undernet.org login TU_USUARIO TU_CLAVE
AWAY :Conectado desde xBNC
```

Para identificarte con X, NickServ, Q o AuthServ es mejor el módulo [Login en servicios](MODULES.md#mod-services): espera a que la red confirme el inicio de sesión antes de entrar en los canales y en Undernet usa Login on Connect.

### Undernet

Undernet usa el servicio **X** y no tiene SASL ni TLS. Activa [Login en servicios](MODULES.md#mod-services) con tu cuenta de X: con *Login on Connect* la cuenta viaja en la contraseña del servidor (`+x! cuenta contraseña`), así que tu IP real no llega a verse nunca y entras en los canales ya con `cuenta.users.undernet.org`.

```text
*** Tu host es ahora y2k.users.undernet.org
* y2k ha entrado en #xbnc
<*status> xBNC v1.0.0 - https://xbnc.org
```

- Con [Invitación y desbaneo](MODULES.md#mod-invite), si no puedes entrar en un canal tuyo, xBNC pide a X `INVITE` o `UNBAN`.
- [Ignorar](MODULES.md#mod-ignore) usa `SILENCE`: los mensajes de quien ignoras ni siquiera llegan a xBNC.
- Los nicks tienen 12 caracteres como máximo; el nick de ausente se recorta solo.
- Muchos canales hispanos usan Latin-1: xBNC lo entiende siempre, y con [Codificación](MODULES.md#mod-charset) puedes escribirles en Latin-1.
- La conexión con Undernet va sin cifrar porque la red no ofrece TLS; la contraseña de X viaja en claro hasta el servidor, igual que con cualquier cliente.

### Libera.Chat y freenode

Identificación por **SASL**: pon tu cuenta y contraseña en la configuración de la red. En Libera también puedes usar un certificado ([Certificado de cliente](MODULES.md#mod-cert)) con SASL EXTERNAL: genera el certificado en el panel, conéctate identificado, escribe `/msg NickServ CERT ADD` y reconecta. Si tu nick está ocupado al conectar, [Login en servicios](MODULES.md#mod-services) con «Recuperar mi nick» envía `REGAIN` (Libera) o `GHOST` (freenode).

### Rizon

NickServ (Anope), por SASL o con Login en servicios. ChanServ atiende las invitaciones y los desbaneos.

### EvilNet

Te identificas con **NickServ** (`IDENTIFY cuenta contraseña`). Elige NickServ en Login en servicios, o «Automático», que en EvilNet usa NickServ.

### QuakeNet

Servicio **Q** (`AUTH` a `Q@CServe.quakenet.org`) y `+x`. Login en servicios lo hace por ti; la invitación y el desbaneo usan `INVITE` y `UNBANME`.

### IRCnet y EFnet

No tienen NickServ: cualquiera podría ponerse ese nick. Por eso, en «Automático», xBNC **nunca envía tu contraseña de servicios** en estas redes y lo avisa en `*status` y en el registro. IRCnet ofrece SASL opcional con cuentas de sasl.ircnet.com. Para conservar op sin servicios, usa [Recuperar op al quedarte solo](MODULES.md#mod-autocycle).

### OFTC y otras redes con NickServ

Login en servicios con NickServ, o comandos al conectar. Para el certificado de cliente, OFTC usa la huella SHA-1 y Libera la SHA-512: el panel muestra las tres huellas (SHA-1, SHA-256 y SHA-512).
