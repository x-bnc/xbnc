# Qué es xBNC

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

- [Qué es xBNC](#intro)
- [Cómo funciona](#how-it-works)

<a id="intro"></a>

## Qué es xBNC

xBNC es un **bouncer IRC**: un programa que se queda conectado a tus redes IRC las 24 horas, aunque cierres el cliente o apagues el ordenador. Cuando vuelves, te entrega lo que te perdiste, una sola vez en cada dispositivo.

Se instala en un servidor propio (un VPS, una Raspberry Pi, un ordenador de casa), se usa con tu cliente IRC de siempre y se configura entero desde un panel web, o desde IRC con `/msg *status`. Es un único programa escrito en Rust, con el panel incluido dentro.

- **24** módulos nativos configurables por red
- **18** plugins oficiales en WebAssembly
- **2+** idiomas: inglés y español incluidos, y cualquier otro con un fichero de traducción

### Qué incluye

- **Bouncer:** conexión permanente a varias redes por usuario, con TLS verificado, SASL PLAIN y EXTERNAL, reconexión automática con espera progresiva y protección anti-flood hacia el servidor.
- **Varios dispositivos a la vez:** lo que escribes en uno aparece en los demás, y cada uno lleva su propia cuenta de lo que ha leído.
- **Historial:** canales y privados con búsqueda de texto completo, filtros, exportación en TXT y JSON, retención general y por canal, y cuotas por usuario.
- **IRCv3 completo** con los servidores y con los clientes ([lista completa](IRCV3.md#ircv3)); los clientes clásicos ven el IRC de siempre.
- **Chat web** en el panel, vista de menciones y marcadores de lectura compartidos entre dispositivos.
- **Avisos** de menciones y privados por Telegram, Discord, correo, una dirección web propia o notificaciones push del navegador y el móvil.
- **24 módulos** (login en servicios, ausente automático, op automático, cifrado FiSH, canales desacoplados, tareas programadas…) y **18 plugins** ya instalados (trivial, encuestas, recordatorios, el tiempo…).
- **Multiusuario:** cinco roles, cuotas, suspensión, auditoría, verificación en dos pasos, passkeys, OpenID Connect y LDAP.
- **Datos:** SQLite o PostgreSQL, alta disponibilidad activo/en espera, copias de seguridad desde el panel o la terminal, migración desde otro bouncer.
- **Administración:** panel web, API REST con tokens, WebSocket, métricas para Prometheus con panel de Grafana, alertas por panel, IRC y webhook.
- **Seguridad:** Argon2id, credenciales cifradas, bloqueo de fuerza bruta, CSRF, CSP, servicio sin privilegios, plugins aislados y *fuzzing* continuo.

> **Versión 1.0.0, beta de pruebas.** Es la primera versión completa y se está probando en redes reales antes de declararla estable. Consulta [Probar la beta](TESTING.md#beta) para participar.

<a id="how-it-works"></a>

## Cómo funciona

```text
Tus dispositivos                      xBNC                          Redes IRC

Portátil · HexChat  ──┐      ┌─────────────────────┐       ┌──▶ Undernet
Móvil    · Goguma   ──┼─────▶│ siempre conectado   │───────┼──▶ Libera.Chat
Navegador · panel   ──┘      │ historial por equipo│       ├──▶ Rizon
(apagado: no se pierde nada) │ módulos y plugins   │       └──▶ IRCnet…
                             │ panel web y API     │
                             └─────────────────────┘
        TLS · usuario/red@dispositivo           TLS · SASL · servicios
```

Tu cliente IRC ya no se conecta a la red, sino a xBNC. xBNC mantiene **una conexión por red** que no se corta cuando cierras el cliente, y se presenta ante cada dispositivo como si fuera el servidor de esa red.

1. **Te conectas** con la contraseña de servidor `usuario/red@dispositivo:contraseña`. El nombre del dispositivo (`@portatil`, `@movil`) es opcional, pero permite que cada uno lleve su propia cuenta de lo que ha leído.
2. **xBNC te pone al día**: entra contigo en tus canales, te envía el tema y la lista de usuarios, y reproduce los mensajes que llegaron desde tu última conexión en ese dispositivo, sin repetir lo que ya viste.
3. **Mientras no estás**, xBNC sigue en los canales, guarda el historial, responde a CTCP, te marca como ausente y ejecuta tus módulos y plugins.
4. **Todos tus dispositivos están sincronizados**: lo que escribes en el móvil aparece en el portátil, y los marcadores de lectura se comparten.

### Qué recibes al conectar

1. La bienvenida de xBNC con los parámetros (ISUPPORT) de la red real.
2. El MOTD de la red (salvo que actives [Ocultar el MOTD](MODULES.md#mod-block_motd)), seguido del nombre y la versión de xBNC.
3. Los canales en los que estás, con su tema y su lista de usuarios.
4. Los mensajes que este dispositivo no ha visto. Si el cliente no admite `server-time`, cada mensaje lleva delante la fecha y la hora entre corchetes.
5. Si te perdiste privados o menciones, un resumen de `*status` (módulo [Ausente automático](MODULES.md#mod-away)).
6. Si la red está desconectada, un aviso de `*status` con el motivo.
7. El mensaje del día de este servidor, si un superadministrador lo ha puesto (`/msg *status setmotd`).

### Historial sin duplicados

- Cada dispositivo (el `@nombre` del login, o `default` si no pones ninguno) tiene un cursor por red: el último mensaje que se le entregó. Dos conexiones con el mismo nombre comparten cursor.
- Al conectar recibe los mensajes posteriores a su cursor, hasta `playback_max` (1000 por defecto). Un dispositivo nuevo recibe los `new_client_playback` más recientes (100 por defecto).
- Los cursores se guardan al desconectar y cada 5 segundos, y nunca retroceden. Si el proceso se corta de golpe, como mucho se repiten unos segundos.
- Los clientes con `draft/chathistory` no reciben reproducción automática: piden lo que necesitan con `CHATHISTORY` (hasta `chathistory_max`, 500 por defecto).

### Conexión con cada red

- Estados: desconectada, registrándose y conectada. Durante el registro se negocian las capacidades IRCv3, SASL y el nick (con alternativa si está ocupado).
- Tiempo máximo de registro: 60 segundos. Si el servidor calla 90 segundos se le envía PING, y a los 180 se da la conexión por perdida.
- Reconexión automática con espera progresiva de 5 segundos a 5 minutos, con una variación aleatoria del ±20 % para que miles de redes no vuelvan en el mismo segundo. Si la red avisa de que reconectas demasiado rápido, xBNC espera al menos 2 minutos.
- La salida hacia el servidor pasa por un control de ritmo (ráfaga de 9 líneas y después 2 por segundo) para que la red no te expulse por flood.
- El certificado TLS del servidor se verifica siempre con los certificados raíz del sistema. Para una red con certificado propio o caducado se puede confiar en ese certificado concreto por su huella SHA-256 (`/msg *status addfingerprint`); no existe opción para desactivar la verificación.
- Cada red puede tener servidores de reserva (`/msg *status addserver`). Si la conexión a un servidor falla, xBNC prueba el siguiente al segundo; tras recorrer la lista entera vuelve a la espera progresiva. Si un servidor ya conectado corta la conexión, xBNC vuelve primero a ese mismo servidor.
- La dirección local de salida (vhost) se puede fijar por red o por usuario (`/msg *status setbindhost`, solo administradores).
- Si una línea no es UTF-8 se interpreta como Windows-1252, así que las tildes de los clientes antiguos se ven bien.

### Usuarios virtuales

xBNC añade «usuarios» que empiezan por `*`. No existen en la red: son la forma de hablar con xBNC desde cualquier cliente. `*status`, `*notes`, `*watch`, `*crypt`, `*ignore` y uno por cada plugin (`*trivia`, `*remind`…). Ver [Comandos IRC](COMMANDS.md#commands).
