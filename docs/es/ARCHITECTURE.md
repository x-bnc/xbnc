<a id="architecture"></a>

# Arquitectura

*Parte de la documentación de xBNC 1.0.0. El mismo contenido, con buscador, está publicado como la wiki de xBNC, una web aparte. Estos ficheros se generan a partir de la wiki; para cambiarlos, cambia la wiki.*

xBNC es un único programa (`xbncd`) con el panel incluido. El código se divide en `crates/xbnc-proto` (protocolo IRC sin E/S: parser y serializador con etiquetas IRCv3, prefijos, casemapping, ISUPPORT y CTCP) y `crates/xbncd` (servicio, CLI, API y panel), más los SDK de plugins en `sdk/`, el panel (React y TypeScript) en `web/` y las traducciones en `locales/`.

### Cómo trabaja por dentro

- Cada red de cada usuario es una tarea independiente que posee en exclusiva su conexión con el servidor, el estado de la red y la lista de clientes. Nadie más toca ese estado: no hay bloqueos en el camino de los mensajes.
- Cada cliente es una tarea de lectura y otra de escritura, con una cola acotada. Si un cliente no lee, se le desconecta en vez de acumular memoria; al volver recupera lo pendiente.
- El panel, la API y la terminal nunca tocan las conexiones IRC: escriben en la base de datos y envían órdenes a las tareas de red (estado, control, entrar, salir, recargar, desconectar un cliente, alerta, aviso a todos).
- Todo acceso a la base de datos va por un grupo de hilos aparte. Los mensajes de todas las redes pasan por un único hilo escritor que guarda en una sola transacción todo lo que espera (hasta 512), conservando el orden de cada red.
- Los módulos son código del propio programa, llamados en puntos concretos (línea del servidor, comando de un cliente, temporizador, conexión, cliente que entra o sale). Los plugins reciben los eventos desde la tarea de red y se ejecutan en el grupo de hilos bloqueante.
- Al llegar un mensaje: se interpreta, se actualiza el estado (nick, canales, miembros, modos, tema), se guarda con la hora del servidor y un identificador, y se envía a cada cliente adaptado a sus capacidades.
- Los textos se buscan por clave en los idiomas cargados (los incluidos y los ficheros de `locales_dir`), con el inglés como respaldo.

### Módulos del código

| Módulo (crates/xbncd/src) | Qué hace |
| --- | --- |
| `main.rs` | El comando `xbncd`: el servicio y la línea de comandos (usuarios, redes, copias, importación, idiomas, versiones, actualizaciones). |
| `config` | Lectura y validación de `xbnc.toml`. |
| `db`, `sql` | Almacenamiento: migraciones versionadas, usuarios, redes, canales, mensajes, cursores, auditoría y copias; `sql` da a SQLite (rusqlite) y a PostgreSQL la misma interfaz. |
| `crypto` | Argon2id, cifrado XChaCha20-Poly1305 de las credenciales guardadas y un tipo `Secret` que nunca se imprime. |
| `auth` | El formato de login, la comprobación de contraseñas, el limitador de fuerza bruta y el enmascarado de líneas en los logs. |
| `util` | Codec de líneas IRC (límite de longitud, UTF-8 con pérdida, protección contra inyección de CR/LF) y funciones de tiempo. |
| `network/` | La tarea de cada red de cada usuario: conexión con el servidor IRC, estado, historial y clientes vinculados. `mods.rs` y `extra.rs` tienen el comportamiento de los módulos, `status.rs` y `status_more.rs` los comandos `*status`, `*notes` y `*watch`, `plugins.rs` entrega los eventos a los plugins, `dcc.rs` decide qué ofertas DCC pasan por xBNC y `readmark.rs` guarda las marcas de lectura. |
| `downstream`, `bouncer`, `events` | Conexiones de clientes: registro, CAP, SASL, PASS y reenvío a la tarea de la red; `soju.im/bouncer-networks`; avisos a los clientes cuando cambia una red. |
| `modules` | Catálogo de módulos, esquema y validación de sus ajustes y configuración tipada por red. |
| `perform` | Convierte comandos al estilo del cliente (`/msg`, `/mode`…) en IRC, para los comandos al conectar, los alias y las tareas programadas. |
| `logs` | Registros en ficheros, escritos en un hilo aparte. |
| `certs` | Certificados de cliente (generación, validación y huellas) y certificados de servidor de confianza. |
| `fish` | Cifrado FiSH: Blowfish ECB y CBC y el intercambio de claves DH1080. |
| `i18n` | Idiomas: inglés y español incluidos y los ficheros de `locales_dir`. |
| `netinfo` | Direcciones de la red local y la dirección que se muestra para `0.0.0.0`. |
| `server`, `ha` | Contexto compartido, registro de tareas de red, puertos TLS, mantenimiento y recarga; alta disponibilidad activo/en espera con PostgreSQL. |
| `rbac`, `settings` | Los cinco roles y sus permisos; ajustes de historial y límites editables en caliente (base de datos sobre `xbnc.toml`). |
| `totp`, `webauthn`, `sso` | Verificación en dos pasos (RFC 6238), passkeys, OpenID Connect y LDAP. |
| `tz` | Zonas horarias con horario de verano, con la base de datos incluida. |
| `metrics`, `alerts` | Contadores de tráfico y mensajes, memoria y CPU del proceso; reglas de alerta comprobadas cada 30 s, con avisos en el panel, por IRC y por webhook. |
| `notify`, `push` | Avisos por Telegram, Discord, correo, una dirección web propia y Web Push. |
| `plugins`, `plugin_dev`, `catalog` | Plugins WebAssembly: manifiesto, validación, motor Wasmi con límites, instancias y acciones; modo desarrollo; el catálogo incluido y el remoto firmado. |
| `dcc` | Lectura y reescritura de ofertas DCC y los túneles de un solo uso. |
| `http` | Cliente HTTP/1.1 mínimo (TLS verificado, tamaño y tiempo acotados) para webhooks, avisos, plugins con acceso web y la comprobación de versiones. |
| `updates`, `release` | Aviso de versiones nuevas, descarga y comprobación de paquetes, el ayudante `xbncd apply-update` y las firmas Ed25519. |
| `backup` | Copias de seguridad desde el panel y su restauración. |
| `import` | Migración desde otro bouncer: lectura de su configuración, datos de módulos, plan, aplicación en una transacción e importación de registros. |
| `web/` | Servidor HTTP (axum): API `/api/v1`, WebSocket, sesiones, CSRF, cabeceras y los ficheros del panel incluidos en el binario. |

El panel (`web/`) es una aplicación React y TypeScript compilada con Vite. `web/dist` se incluye en el binario, así que xBNC sigue siendo un único fichero. Los plugins oficiales también van dentro del binario (`crates/xbncd/catalog/`).

### Máquinas de estado

Se mantienen separadas:

1. **Conexión con la red**: `Disconnected → Registering → Connected`. Durante el registro, xBNC negocia las capacidades IRCv3, SASL y el nick (con alternativa si está ocupado). Tiempos: 30 s para conectar, 60 s para registrarse, PING tras 90 s de silencio y desconexión a los 180 s. Reconexión con esperas crecientes de 5 s a 5 minutos con una variación aleatoria de ±20 %, y al menos 2 minutos tras un aviso de *throttling*. Si un servidor falla y la red tiene servidores de reserva, se prueba el siguiente al cabo de 1 segundo.
2. **Sesión de cliente**: registro (`CAP`, `PASS` o SASL, `NICK`, `USER`) con tiempo límite, y después una sesión vinculada a una red.
3. **Usuario y bouncer**: qué tareas de red existen y cuántos clientes tiene cada usuario.

### El camino de un mensaje

1. La tarea de la red lee una línea del servidor y la analiza.
2. Actualiza el estado (nick, canales, miembros, modos, tema).
3. Si es un PRIVMSG, NOTICE o ACTION de alguien (o un evento, con el módulo [Eventos](MODULES.md#mod-events)), lo guarda en `messages` con la hora del servidor (`server-time`) o la local, y le da el identificador `msgid = x<id>`.
4. Lo envía a cada cliente vinculado adaptado a sus capacidades (quitando las tags que el cliente no pidió) y avanza el cursor de cada cliente.

Lo que envía un cliente va al servidor, se guarda como saliente y se reenvía al resto de clientes del usuario (y al propio emisor si pidió `echo-message`).

### Historial sin duplicados

- Cada dispositivo (`@dispositivo` en el login, `default` si no se indica) tiene un cursor por red en `client_cursors`: el identificador del último mensaje entregado.
- Al conectar, recibe los mensajes posteriores a su cursor (hasta `playback_max`, 1.000 por defecto). Un dispositivo nuevo recibe los `new_client_playback` más recientes (100 por defecto).
- Los cursores se guardan al desconectar el dispositivo y cada 5 segundos; nunca retroceden.
- Los clientes con `draft/chathistory` no reciben reproducción automática: piden lo que necesitan con `CHATHISTORY`. Los mensajes ya leídos en otro dispositivo (`draft/read-marker`) no se vuelven a reproducir.

### Esquema de la base de datos

| Tabla | Contenido | Desde la migración |
| --- | --- | --- |
| `users` | Usuario, hash Argon2id, rol, límites y cuotas, secreto TOTP cifrado, idioma, zona horaria, dirección de salida, último acceso y última conexión. | 1 (2, 4, 8, 9, 10) |
| `networks` | Servidor, puerto, TLS, nick, SASL, comandos al conectar, certificado de cliente, servidores de reserva y certificados de confianza; contraseñas y claves cifradas. | 1 (3, 4, 10) |
| `channels` | Canales guardados por red, con clave cifrada, retención, desactivado y posición. | 1 (8, 10) |
| `messages` | Historial: red, conversación, hora, emisor, comando, texto, saliente y mención. | 1 (8) |
| `client_cursors` | Último mensaje entregado por red y dispositivo. | 1 |
| `audit_log` | Acciones administrativas. | 1 |
| `web_sessions` | Sesiones del panel (hash del token, CSRF, IP, navegador, caducidad). | 2 |
| `settings` | Ajustes cambiados desde el panel. | 2 |
| `login_events` | Intentos de acceso por IRC y en el panel (10.000 como máximo). | 2 |
| `module_config` | Módulos por red: activo y ajustes en JSON validado. | 4 |
| `notes` | Notas de cada usuario. | 4 |
| `messages_fts` | Índice de texto completo de `messages.text`, mantenido con *triggers* (SQLite; PostgreSQL usa su propia búsqueda de texto). | 5 |
| `api_tokens` | Tokens de API: hash SHA-256, prefijo, alcance, caducidad, último uso e IP. | 6 |
| `plugins` | Plugins instalados: manifiesto, SHA-256, tamaño, activo en el servidor, dominios web aprobados y el propio módulo. | 7 (8) |
| `plugin_config` | Plugins por red: activo, ajustes y estado guardado. | 7 |
| `plugin_kv` | Almacén clave-valor de cada plugin por red. | 8 |
| `notify_settings` | Ajustes de avisos de cada usuario. | 8 |
| `push_subscriptions` | Suscripciones Web Push de los navegadores de cada usuario. | 8 |
| `read_markers` | Último mensaje leído de cada conversación. | 8 |
| `passkeys` | Passkeys: credencial, clave pública, contador, nombre y último uso. | 8 |
| `user_identities` | Identidades externas (OpenID Connect, LDAP) vinculadas a usuarios locales. | 8 |

PostgreSQL tiene el mismo esquema; se admite desde la versión 8 del esquema (`PG_MIGRATIONS`) y cada migración nueva se añade a las dos listas. Los eventos de canal (JOIN, PART…) se guardan en `messages` con su comando. Las contraseñas importadas de otro bouncer con SHA-256 se guardan como `legacy-sha256$<hash>$<sal>` hasta el primer acceso correcto, en el que se sustituyen por Argon2id. Las migraciones se aplican al arrancar dentro de una transacción, y un binario se niega a abrir un esquema más nuevo del que conoce.
